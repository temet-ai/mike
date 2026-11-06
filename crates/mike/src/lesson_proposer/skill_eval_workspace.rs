use std::cmp::Reverse;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::process::Command;

use super::progress::Progress;
use super::{EvalError, Material};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::lessons::skill_draft_eval::EvalDraft;

const MAX_FILES: usize = 60;
const MAX_TOP_LEVEL: usize = 40;
const MAX_WORD_FILES: usize = 80;
const MIN_WORD_CHARS: usize = 5;
const LIST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Workspace {
    pub path: String,
    pub available: bool,
    pub tracked: usize,
    pub top_level: Vec<String>,
    pub files: Vec<String>,
}

#[derive(Debug, Default)]
struct Wanted {
    paths: BTreeSet<String>,
    words: HashSet<String>,
}

pub(super) async fn load(path: &str, material: &Material) -> Workspace {
    let Some(tracked) = tracked_files(path).await else {
        return Workspace {
            path: path.to_owned(),
            ..Workspace::default()
        };
    };
    Workspace {
        path: path.to_owned(),
        available: true,
        tracked: tracked.len(),
        top_level: top_level(&tracked),
        files: relevant(&tracked, &material_text(material)),
    }
}

async fn tracked_files(path: &str) -> Option<Vec<String>> {
    if path.trim().is_empty() || !tokio::fs::metadata(path).await.is_ok_and(|m| m.is_dir()) {
        return None;
    }
    let output = run_listing(path).await?;
    let text = String::from_utf8_lossy(&output.stdout);
    Some(
        text.split('\0')
            .filter(|file| !file.is_empty())
            .map(str::to_owned)
            .collect(),
    )
}

async fn run_listing(path: &str) -> Option<std::process::Output> {
    let listing = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["ls-files", "-z"])
        .kill_on_drop(true)
        .output();
    let output = match tokio::time::timeout(LIST_TIMEOUT, listing).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => return not_listed(path, &error.to_string()),
        Err(_) => return not_listed(path, "git ls-files timed out"),
    };
    if !output.status.success() {
        return not_listed(path, &format!("git ls-files {}", output.status));
    }
    Some(output)
}

fn not_listed<T>(path: &str, reason: &str) -> Option<T> {
    tracing::warn!(path, reason, "skill eval: project files not listed");
    None
}

fn material_text(material: &Material) -> String {
    let mut text = String::new();
    text.push_str(&material.skill_md);
    for reference in &material.references {
        text.push('\n');
        text.push_str(&reference.content);
    }
    for query in &material.queries {
        let _ = write!(text, "\n{}", query.query);
    }
    text
}

pub(super) async fn ensure(
    persist: &PersistHandle,
    draft: &EvalDraft,
    material: &Material,
    progress: &mut Progress,
) -> Result<(), EvalError> {
    if progress.workspace.is_some() {
        return Ok(());
    }
    let path = persist
        .skill_draft_project_path(draft.project_id)
        .await
        .map_err(|e| EvalError::Deferred(format!("project path: {e}")))?;
    progress.workspace = Some(load(path.as_deref().unwrap_or_default(), material).await);
    Ok(())
}

pub(super) fn text_of(progress: &Progress) -> String {
    progress.workspace.as_ref().map(context).unwrap_or_default()
}

pub(super) fn top_level(files: &[String]) -> Vec<String> {
    let entries: BTreeSet<String> = files
        .iter()
        .map(|file| match file.split_once('/') {
            Some((dir, _)) => format!("{dir}/"),
            None => file.clone(),
        })
        .collect();
    entries.into_iter().take(MAX_TOP_LEVEL).collect()
}

fn has_extension(token: &str) -> bool {
    token.rsplit_once('.').is_some_and(|(stem, extension)| {
        !stem.is_empty()
            && (2..=6).contains(&extension.len())
            && extension.chars().all(|c| c.is_ascii_alphabetic())
    })
}

fn wanted(text: &str) -> Wanted {
    let mut found = Wanted::default();
    let path_chars =
        |c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | '-' | '+'));
    for raw in text.split(path_chars) {
        let token = raw.trim_matches(|c| matches!(c, '.' | '/' | '-'));
        if token.len() >= 4 && (token.contains('/') || has_extension(token)) {
            found.paths.insert(token.to_owned());
            continue;
        }
        for part in token.split(|c: char| !c.is_ascii_alphanumeric()) {
            if part.len() >= MIN_WORD_CHARS {
                found.words.insert(part.to_ascii_lowercase());
            }
        }
    }
    found
}

fn components(file: &str) -> HashSet<String> {
    let stem = match file.rsplit_once('.') {
        Some((stem, extension)) if !extension.contains('/') => stem,
        _ => file,
    };
    stem.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| part.len() >= MIN_WORD_CHARS)
        .map(str::to_ascii_lowercase)
        .collect()
}

fn common_words(files: &[String], wanted: &Wanted) -> HashSet<String> {
    let mut hits: HashMap<String, usize> = HashMap::new();
    for file in files {
        for part in components(file) {
            if wanted.words.contains(&part) {
                *hits.entry(part).or_default() += 1;
            }
        }
    }
    hits.into_iter()
        .filter(|(_, files)| *files > MAX_WORD_FILES)
        .map(|(word, _)| word)
        .collect()
}

fn path_weight(file: &str, token: &str) -> usize {
    if token.contains('/') {
        return if file.contains(token) { 3 } else { 0 };
    }
    if file.rsplit('/').next() == Some(token) {
        2
    } else {
        0
    }
}

fn score(file: &str, wanted: &Wanted, common: &HashSet<String>) -> usize {
    let by_path: usize = wanted.paths.iter().map(|t| path_weight(file, t)).sum();
    let by_word = components(file)
        .iter()
        .filter(|part| wanted.words.contains(*part) && !common.contains(*part))
        .count();
    by_path + by_word
}

pub(super) fn relevant(files: &[String], text: &str) -> Vec<String> {
    let wanted = wanted(text);
    let common = common_words(files, &wanted);
    let mut scored: Vec<(usize, &String)> = files
        .iter()
        .map(|file| (score(file, &wanted, &common), file))
        .filter(|(score, _)| *score > 0)
        .collect();
    scored.sort_by_key(|(score, file)| (Reverse(*score), file.len(), (*file).clone()));
    let mut chosen: Vec<String> = scored
        .into_iter()
        .take(MAX_FILES)
        .map(|(_, file)| file.clone())
        .collect();
    chosen.sort();
    chosen
}

pub(super) fn context(workspace: &Workspace) -> String {
    if !workspace.available {
        return format!(
            "The skills and messages here belong to the project at {}. Its files cannot be \
             listed in this evaluation. Any other files you may notice around you, such as \
             schema.json, belong to the tool that runs you and are not part of the project.",
            workspace.path
        );
    }
    let mut out = format!(
        "The skills and messages here belong to the project at {}, a real checkout with {} \
         tracked files. Any other files you may notice around you, such as schema.json, belong \
         to the tool that runs you and are not part of the project. Top level: {}\n",
        workspace.path,
        workspace.tracked,
        workspace.top_level.join(", ")
    );
    if !workspace.files.is_empty() {
        out.push_str(
            "\nFiles that the draft and its trigger messages mention (a selection, not the full list):\n",
        );
        for file in &workspace.files {
            let _ = writeln!(out, "- {file}");
        }
    }
    out
}

#[cfg(test)]
#[path = "skill_eval_workspace_tests.rs"]
mod tests;
