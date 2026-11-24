use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;
use uuid::Uuid;

use super::declared::{self, DeclaredSource};
use super::files::{self, ScanError, Scope, SourceKind, hex_digest};
use super::machine::{self, CHECKOUT_INSTRUCTIONS, INSTRUCTIONS_SOURCE};
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
    missing_reported: Vec<(Uuid, String, PathBuf)>,
    machine_missing_reported: Vec<(String, PathBuf)>,
}

impl ScanMemo {
    pub(super) fn forget(&mut self, owner: Uuid, name: &str) {
        self.submitted.remove(&(owner, name.to_owned()));
    }

    /// Records the machine sources whose roots are missing now. Returns false when
    /// that set is the one already reported, so the report is not sent again.
    pub(super) fn note_machine_missing(&mut self, mut missing: Vec<(String, PathBuf)>) -> bool {
        missing.sort();
        if missing == self.machine_missing_reported {
            return false;
        }
        self.machine_missing_reported = missing;
        true
    }
}

pub(super) struct Spec {
    pub name: String,
    pub kind: SourceKind,
    pub root: PathBuf,
    pub scope: Scope,
    pub declared: bool,
}

impl From<DeclaredSource> for Spec {
    fn from(source: DeclaredSource) -> Self {
        Self {
            name: source.name,
            kind: SourceKind::Tree,
            root: source.root,
            scope: Scope {
                excludes: source.excludes,
                ..Scope::default()
            },
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

fn builtin_specs(root_path: &str, claude_home: &Path) -> Vec<Spec> {
    let home_of_claude = Path::new(root_path).join(".claude") == claude_home;
    let memory = Spec {
        name: MEMORY_SOURCE.to_owned(),
        kind: SourceKind::Memory,
        root: claude_home
            .join("projects")
            .join(encode_project_dir(root_path))
            .join("memory"),
        scope: Scope::default(),
        declared: false,
    };
    let instructions = Spec {
        name: INSTRUCTIONS_SOURCE.to_owned(),
        kind: SourceKind::Instructions,
        root: PathBuf::from(root_path),
        scope: Scope {
            includes: location_instructions(home_of_claude),
            ..Scope::default()
        },
        declared: false,
    };
    if home_of_claude {
        return vec![memory, instructions];
    }
    let skills = Spec {
        name: SKILLS_SOURCE.to_owned(),
        kind: SourceKind::Skills,
        root: Path::new(root_path).join(".claude").join("skills"),
        scope: Scope::default(),
        declared: false,
    };
    vec![memory, skills, instructions]
}

fn location_instructions(home_of_claude: bool) -> Vec<String> {
    CHECKOUT_INSTRUCTIONS
        .iter()
        .filter(|path| !(home_of_claude && path.starts_with(".claude/")))
        .map(|path| (*path).to_owned())
        .collect()
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
    let machine_plan = machine::plan(claude_home).await;
    let mut report = ScanReport {
        scanned: plan.present.len() + machine_plan.present.len(),
        failed: plan.unreadable_declarations,
        ..ScanReport::default()
    };
    report.failed += record_missing_roots(persist, &plan.missing, memo).await?;
    let registered = persist
        .knowledge()
        .register_knowledge_sources(&source_inputs(plan.present.iter()))
        .await?;
    let machine_registered =
        register_machine_sources(persist, machine, &machine_plan, memo, &mut report).await;
    let mut collected = Vec::new();
    let locations = plan
        .present
        .iter()
        .map(|(location, spec)| (*location, spec));
    sync_batch(
        persist,
        locations,
        registered,
        memo,
        &mut collected,
        &mut report,
    )
    .await;
    let owned = machine_plan.present.iter().map(|spec| (machine, spec));
    sync_batch(
        persist,
        owned,
        machine_registered,
        memo,
        &mut collected,
        &mut report,
    )
    .await;
    persist
        .knowledge()
        .record_knowledge_source_heartbeats(&collected)
        .await?;
    Ok(report)
}

async fn register_machine_sources(
    persist: &PersistHandle,
    machine: Uuid,
    machine_plan: &machine::MachinePlan,
    memo: &mut ScanMemo,
    report: &mut ScanReport,
) -> Vec<Option<RegisteredSource>> {
    let registered = async {
        let missing =
            machine::record_missing(persist, machine, &machine_plan.missing, memo).await?;
        let sources = persist
            .knowledge()
            .register_machine_knowledge_sources(&machine::inputs(
                machine,
                machine_plan.present.iter(),
            ))
            .await?;
        Ok::<_, PersistError>((missing, sources))
    }
    .await;
    match registered {
        Ok((missing, sources)) => {
            report.failed += missing;
            sources
        }
        Err(error) => {
            report.failed += machine_plan.present.len();
            tracing::warn!(%error, "knowledge_scan: machine sources not registered; checkout sources still scanned");
            Vec::new()
        }
    }
}

async fn sync_batch<'a>(
    persist: &PersistHandle,
    targets: impl Iterator<Item = (Uuid, &'a Spec)>,
    registered: Vec<Option<RegisteredSource>>,
    memo: &mut ScanMemo,
    collected: &mut Vec<Uuid>,
    report: &mut ScanReport,
) {
    for ((owner, spec), source) in targets.zip(registered) {
        match sync_source(persist, owner, spec, source, memo, collected).await {
            Ok(true) => report.updated += 1,
            Ok(false) => {}
            Err(error) => {
                report.failed += 1;
                tracing::warn!(root = %spec.root.display(), source = %spec.name, %error,
                    "knowledge_scan: source not updated; the earlier index is kept");
            }
        }
    }
}

async fn record_missing_roots(
    persist: &PersistHandle,
    missing: &[(Uuid, Spec)],
    memo: &mut ScanMemo,
) -> Result<usize, PersistError> {
    let mut reported: Vec<(Uuid, String, PathBuf)> = missing
        .iter()
        .map(|(location, spec)| (*location, spec.name.clone(), spec.root.clone()))
        .collect();
    reported.sort();
    if reported == memo.missing_reported {
        return Ok(0);
    }
    if missing.is_empty() {
        memo.missing_reported = reported;
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
    memo.missing_reported = reported;
    Ok(usize::try_from(recorded).unwrap_or(usize::MAX))
}

async fn plan_sources(
    persist: &PersistHandle,
    machine: Uuid,
    claude_home: &Path,
) -> Result<Plan, PersistError> {
    let mut plan = Plan::default();
    for location in persist.knowledge().locations_on_machine(machine).await? {
        let mut specs = builtin_specs(&location.root_path, claude_home);
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
    let scope = spec.scope.clone();
    let key = (location, spec.name.clone());
    let collecting = tokio::task::spawn_blocking(move || files::collect(&root, kind, &scope));
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
    if persist
        .knowledge()
        .scan_fingerprint_matches(source_uuid, generation, &print)
        .await?
    {
        memo.submitted.insert(key, (generation, print));
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
    persist
        .knowledge()
        .record_scan_fingerprint(source_uuid, next, &print)
        .await?;
    memo.submitted.insert(key, (next, print));
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::{builtin_specs, encode_project_dir};
    use crate::knowledge_scan::files::SourceKind;
    use std::path::{Path, PathBuf};

    #[test]
    fn builtin_specs_include_a_checkout_instructions_source_limited_to_the_named_paths() {
        let specs = builtin_specs("/work/repo", Path::new("/home/x/.claude"));
        let instructions = specs
            .iter()
            .find(|spec| spec.name == "instructions")
            .map(|spec| (spec.kind, spec.root.as_path(), spec.scope.includes.clone()));
        assert_eq!(
            instructions,
            Some((
                SourceKind::Instructions,
                Path::new("/work/repo"),
                vec![
                    "CLAUDE.md".to_owned(),
                    "AGENTS.md".to_owned(),
                    ".claude/rules".to_owned(),
                    ".claude/agents".to_owned(),
                ]
            ))
        );
    }

    #[test]
    fn builtin_specs_home_location_leaves_machine_sources_to_the_machine() {
        let specs = builtin_specs("/home/x", Path::new("/home/x/.claude"));
        let names: Vec<&str> = specs.iter().map(|spec| spec.name.as_str()).collect();
        assert_eq!(names, vec!["claude-memory", "instructions"]);
        assert_eq!(
            specs[1].scope.includes,
            vec!["CLAUDE.md".to_owned(), "AGENTS.md".to_owned()]
        );
        assert_eq!(
            specs[0].root,
            Path::new("/home/x/.claude/projects/-home-x/memory")
        );
    }

    #[test]
    fn missing_machine_roots_are_reported_only_when_the_set_changes() {
        let mut memo = super::ScanMemo::default();
        let gone = || vec![("skills".to_owned(), PathBuf::from("/h/.claude/skills"))];
        assert!(memo.note_machine_missing(gone()), "first sighting reports");
        assert!(
            !memo.note_machine_missing(gone()),
            "the same set is not reported again"
        );
        assert!(memo.note_machine_missing(Vec::new()), "the root came back");
        assert!(memo.note_machine_missing(gone()), "and went away again");
    }

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
