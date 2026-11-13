use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;
use uuid::Uuid;

use super::declared::{self, DeclaredSource};
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
    submitted: HashMap<(Uuid, String), (i64, String)>,
}

struct Spec {
    name: String,
    kind: SourceKind,
    root: PathBuf,
    excludes: Vec<String>,
    declared: bool,
}

impl From<DeclaredSource> for Spec {
    fn from(source: DeclaredSource) -> Self {
        Self {
            name: source.name,
            kind: SourceKind::Tree,
            root: source.root,
            excludes: source.excludes,
            declared: true,
        }
    }
}

#[derive(Default)]
struct Plan {
    present: Vec<(Uuid, Spec)>,
    missing: Vec<(Uuid, Spec)>,
    unreadable_declarations: usize,
}

pub fn encode_project_dir(root_path: &str) -> String {
    root_path
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn builtin_specs(root_path: &str, claude_home: &Path) -> [Spec; 2] {
    [
        Spec {
            name: MEMORY_SOURCE.to_owned(),
            kind: SourceKind::Memory,
            root: claude_home
                .join("projects")
                .join(encode_project_dir(root_path))
                .join("memory"),
            excludes: Vec::new(),
            declared: false,
        },
        Spec {
            name: SKILLS_SOURCE.to_owned(),
            kind: SourceKind::Skills,
            root: Path::new(root_path).join(".claude").join("skills"),
            excludes: Vec::new(),
            declared: false,
        },
    ]
}

fn source_inputs<'a>(targets: impl Iterator<Item = &'a (Uuid, Spec)>) -> Vec<SourceInput> {
    targets
        .map(|(location, spec)| SourceInput {
            location_uuid: *location,
            name: spec.name.clone(),
            root_path: spec.root.to_string_lossy().into_owned(),
        })
        .collect()
}

pub async fn scan_machine(
    persist: &PersistHandle,
    machine: Uuid,
    claude_home: &Path,
    memo: &mut ScanMemo,
) -> Result<ScanReport, PersistError> {
    let plan = plan_sources(persist, machine, claude_home).await?;
    let mut report = ScanReport {
        scanned: plan.present.len(),
        failed: plan.unreadable_declarations,
        ..ScanReport::default()
    };
    report.failed += record_missing_roots(persist, &plan.missing, memo).await?;
    let registered = persist
        .knowledge()
        .register_knowledge_sources(&source_inputs(plan.present.iter()))
        .await?;
    let mut collected = Vec::new();
    for ((location, spec), source) in plan.present.iter().zip(registered) {
        match sync_source(persist, *location, spec, source, memo, &mut collected).await {
            Ok(true) => report.updated += 1,
            Ok(false) => {}
            Err(error) => {
                report.failed += 1;
                tracing::warn!(root = %spec.root.display(), source = %spec.name, %error,
                    "knowledge_scan: source not updated; the earlier index is kept");
            }
        }
    }
    persist
        .knowledge()
        .record_knowledge_source_heartbeats(&collected)
        .await?;
    Ok(report)
}

async fn record_missing_roots(
    persist: &PersistHandle,
    missing: &[(Uuid, Spec)],
    memo: &mut ScanMemo,
) -> Result<usize, PersistError> {
    if missing.is_empty() {
        return Ok(0);
    }
    for (location, spec) in missing {
        memo.submitted.remove(&(*location, spec.name.clone()));
        tracing::debug!(root = %spec.root.display(), source = %spec.name,
            "knowledge_scan: source root does not exist; the earlier index is kept");
    }
    let declared = source_inputs(missing.iter().filter(|(_, spec)| spec.declared));
    if !declared.is_empty() {
        persist
            .knowledge()
            .register_knowledge_sources(&declared)
            .await?;
    }
    let recorded = persist
        .knowledge()
        .record_knowledge_roots_missing(&source_inputs(missing.iter()))
        .await?;
    Ok(usize::try_from(recorded).unwrap_or(usize::MAX))
}

async fn plan_sources(
    persist: &PersistHandle,
    machine: Uuid,
    claude_home: &Path,
) -> Result<Plan, PersistError> {
    let mut plan = Plan::default();
    for location in persist.knowledge().locations_on_machine(machine).await? {
        let mut specs = Vec::from(builtin_specs(&location.root_path, claude_home));
        match declared::read(&location.root_path).await {
            Ok(found) => specs.extend(found.into_iter().map(Spec::from)),
            Err(error) => {
                plan.unreadable_declarations += 1;
                tracing::warn!(root = %location.root_path, %error,
                    "knowledge_scan: declared sources not scanned");
            }
        }
        for spec in specs {
            let present = tokio::fs::metadata(&spec.root)
                .await
                .is_ok_and(|meta| meta.is_dir());
            let bucket = if present {
                &mut plan.present
            } else {
                &mut plan.missing
            };
            bucket.push((location.location_uuid, spec));
        }
    }
    Ok(plan)
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
    let excludes = spec.excludes.clone();
    let key = (location, spec.name.clone());
    let collecting =
        tokio::task::spawn_blocking(move || files::collect_excluding(&root, kind, &excludes));
    let documents = match collecting.await? {
        Ok(documents) => documents,
        Err(error) => {
            memo.submitted.remove(&key);
            persist
                .knowledge()
                .record_knowledge_source_failure(source_uuid, &error.to_string())
                .await?;
            return Err(error.into());
        }
    };
    collected.push(source_uuid);
    let print = fingerprint(&documents);
    if memo.submitted.get(&key) == Some(&(generation, print.clone())) {
        return Ok(false);
    }
    let result = persist
        .knowledge()
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
