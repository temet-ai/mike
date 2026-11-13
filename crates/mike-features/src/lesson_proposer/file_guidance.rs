use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::json;

use super::ProposerDeps;
use crate::store::persistence::project_knowledge::{AGENTS_SOURCE, GuidanceRow, RULES_SOURCE};

const FILES_SHOWN: usize = 2;
const MIN_COVERAGE: f64 = 0.4;
const MIN_WORD_CHARS: usize = 4;
const MAX_FILES_PER_DIRECTORY: usize = 300;
const README: &str = "README.md";
const FILLER_WORDS: [&str; 12] = [
    "that", "this", "with", "from", "when", "then", "have", "should", "before", "after", "every",
    "does",
];

struct FileDocument {
    source: &'static str,
    relative_path: String,
    title: String,
    body: String,
}

fn words(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|word| {
            word.chars().count() >= MIN_WORD_CHARS && !FILLER_WORDS.contains(&word.as_str())
        })
        .collect()
}

fn coverage(lesson: &BTreeSet<String>, document: &BTreeSet<String>) -> f64 {
    if lesson.is_empty() {
        return 0.0;
    }
    lesson.intersection(document).count() as f64 / lesson.len() as f64
}

fn rank(lesson_text: &str, documents: Vec<FileDocument>) -> Vec<GuidanceRow> {
    let lesson = words(lesson_text);
    let mut scored: Vec<(f64, FileDocument)> = documents
        .into_iter()
        .map(|document| (coverage(&lesson, &words(&document.body)), document))
        .filter(|(score, _)| *score >= MIN_COVERAGE)
        .collect();
    scored.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then_with(|| a.1.relative_path.cmp(&b.1.relative_path))
    });
    scored
        .into_iter()
        .take(FILES_SHOWN)
        .map(|(score, document)| GuidanceRow {
            reference: format!("file:{}", document.relative_path),
            source_name: document.source.to_owned(),
            relative_path: document.relative_path,
            title: document.title.clone(),
            body: document.body,
            metadata: json!({"name": document.title}),
            similarity: Some(score),
        })
        .collect()
}

async fn read_directory(
    root: &Path,
    folder: &str,
    source: &'static str,
    found: &mut Vec<FileDocument>,
) {
    let directory = root.join(".claude").join(folder);
    let Ok(mut entries) = tokio::fs::read_dir(&directory).await else {
        return;
    };
    let mut taken = 0;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path: PathBuf = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".md") || name == README {
            continue;
        }
        taken += 1;
        if taken > MAX_FILES_PER_DIRECTORY {
            break;
        }
        match tokio::fs::read_to_string(&path).await {
            Ok(body) => found.push(FileDocument {
                source,
                relative_path: format!(".claude/{folder}/{name}"),
                title: name.trim_end_matches(".md").to_owned(),
                body,
            }),
            Err(error) => tracing::debug!(%error, path = %path.display(),
                "lesson_proposer: guidance file not readable"),
        }
    }
}

pub async fn closest(
    deps: &ProposerDeps<'_>,
    project_id: i64,
    text: &str,
) -> Vec<GuidanceRow> {
    let Ok(machine) = crate::machine_identity::load_default().await else {
        return Vec::new();
    };
    let roots = match deps
        .persist
        .knowledge()
        .project_roots_on_machine(project_id, machine.machine_uuid)
        .await
    {
        Ok(roots) => roots,
        Err(error) => {
            tracing::warn!(%error, project_id,
                "lesson_proposer: project checkout lookup for rules and agents failed");
            return Vec::new();
        }
    };
    let mut documents = Vec::new();
    for root in &roots {
        let root = Path::new(root);
        read_directory(root, "rules", RULES_SOURCE, &mut documents).await;
        read_directory(root, "agents", AGENTS_SOURCE, &mut documents).await;
        if !documents.is_empty() {
            break;
        }
    }
    rank(text, documents)
}

#[cfg(test)]
#[path = "file_guidance_tests.rs"]
pub mod tests;
