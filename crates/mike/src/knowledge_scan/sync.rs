use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;
use uuid::Uuid;

use super::files::{self, ScanError, SourceKind, hex_digest};
use crate::store::persistence::project_knowledge::{
    MEMORY_SOURCE, RegisteredSource, SKILLS_SOURCE, ScanInput, SourceInput,
};
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error(transparent)]
    Persist(#[from] PersistError),
    #[error(transparent)]
    Scan(#[from] ScanError),
    #[error("the scan task failed: {0}")]
    Task(#[from] tokio::task::JoinError),
    #[error("the source name already refers to a different root")]
    RootConflict,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ScanReport {
    pub scanned: usize,
    pub updated: usize,
    pub failed: usize,
}

impl ScanReport {
    pub fn changed(&self) -> bool {
        self.updated > 0 || self.failed > 0
    }
}

#[derive(Debug, Default)]
pub struct ScanMemo {
    submitted: HashMap<(Uuid, &'static str), (i64, String)>,
}

struct Spec {
    name: &'static str,
    kind: SourceKind,
    root: PathBuf,
}

pub(super) fn encode_project_dir(root_path: &str) -> String {
    root_path
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn specs_for(root_path: &str, claude_home: &Path) -> [Spec; 2] {
    [
        Spec {
            name: MEMORY_SOURCE,
            kind: SourceKind::Memory,
            root: claude_home
                .join("projects")
                .join(encode_project_dir(root_path))
                .join("memory"),
        },
        Spec {
            name: SKILLS_SOURCE,
            kind: SourceKind::Skills,
            root: Path::new(root_path).join(".claude").join("skills"),
        },
    ]
}

pub async fn scan_machine(
    persist: &PersistHandle,
    machine: Uuid,
    claude_home: &Path,
    memo: &mut ScanMemo,
) -> Result<ScanReport, PersistError> {
    let targets = present_sources(persist, machine, claude_home).await?;
    let inputs: Vec<SourceInput> = targets
        .iter()
        .map(|(location, spec)| SourceInput {
            location_uuid: *location,
            name: spec.name.to_owned(),
            root_path: spec.root.to_string_lossy().into_owned(),
        })
        .collect();
    let registered = persist.register_knowledge_sources(&inputs).await?;
    let mut report = ScanReport {
        scanned: targets.len(),
        ..ScanReport::default()
    };
    let mut collected = Vec::new();
    for ((location, spec), source) in targets.iter().zip(registered) {
        match sync_source(persist, *location, spec, source, memo, &mut collected).await {
            Ok(true) => report.updated += 1,
            Ok(false) => {}
            Err(error) => {
                report.failed += 1;
                tracing::warn!(root = %spec.root.display(), source = spec.name, %error,
                    "knowledge_scan: source not updated; the earlier index is kept");
            }
        }
    }
    persist
        .record_knowledge_source_heartbeats(&collected)
        .await?;
    Ok(report)
}

async fn present_sources(
    persist: &PersistHandle,
    machine: Uuid,
    claude_home: &Path,
) -> Result<Vec<(Uuid, Spec)>, PersistError> {
    let mut targets = Vec::new();
    for location in persist.locations_on_machine(machine).await? {
        for spec in specs_for(&location.root_path, claude_home) {
            if tokio::fs::metadata(&spec.root)
                .await
                .is_ok_and(|meta| meta.is_dir())
            {
                targets.push((location.location_uuid, spec));
            }
        }
    }
    Ok(targets)
}

fn fingerprint(
    documents: &[crate::store::persistence::project_knowledge::DocumentInput],
) -> String {
    let mut text = String::new();
    for document in documents {
        text.push_str(&document.relative_path);
        text.push('\0');
        text.push_str(&document.content_hash);
        text.push('\n');
    }
    hex_digest(text.as_bytes())
}

async fn sync_source(
    persist: &PersistHandle,
    location: Uuid,
    spec: &Spec,
    source: Option<RegisteredSource>,
    memo: &mut ScanMemo,
    collected: &mut Vec<Uuid>,
) -> Result<bool, SyncError> {
    let RegisteredSource {
        source_uuid,
        generation,
    } = source.ok_or(SyncError::RootConflict)?;
    let root = spec.root.clone();
    let kind = spec.kind;
    let documents = match tokio::task::spawn_blocking(move || files::collect(&root, kind)).await? {
        Ok(documents) => documents,
        Err(error) => {
            persist
                .record_knowledge_source_failure(source_uuid, &error.to_string())
                .await?;
            return Err(error.into());
        }
    };
    collected.push(source_uuid);
    let print = fingerprint(&documents);
    let key = (location, spec.name);
    if memo.submitted.get(&key) == Some(&(generation, print.clone())) {
        return Ok(false);
    }
    let result = persist
        .ingest_knowledge_scan(
            source_uuid,
            &ScanInput {
                generation,
                documents,
            },
        )
        .await?;
    let next = result
        .get("generation")
        .and_then(Value::as_i64)
        .unwrap_or(generation + 1);
    memo.submitted.insert(key, (next, print));
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::encode_project_dir;

    #[test]
    fn encode_project_dir_replaces_every_non_alphanumeric_character() {
        assert_eq!(
            encode_project_dir("/Users/vimes/projects/temet/temet-proxy"),
            "-Users-vimes-projects-temet-temet-proxy"
        );
        assert_eq!(
            encode_project_dir("/mnt/data/projects/a.b_c"),
            "-mnt-data-projects-a-b-c"
        );
    }
}
