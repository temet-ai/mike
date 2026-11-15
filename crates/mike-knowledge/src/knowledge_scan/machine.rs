use std::path::Path;

use uuid::Uuid;

use super::files::{Scope, SourceKind};
use super::sync::{ScanMemo, Spec};
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::project_knowledge::{
    AGENTS_SOURCE, MachineSourceInput, RULES_SOURCE, SKILLS_SOURCE,
};
use crate::store::persistence::{PersistError, PersistHandle};

pub const AGENT_MEMORY_SOURCE: &str = "agent-memory";
pub const INSTRUCTIONS_SOURCE: &str = "instructions";

const GLOBAL_INSTRUCTIONS_FILE: &str = "CLAUDE.md";

pub(super) const CHECKOUT_INSTRUCTIONS: [&str; 4] =
    ["CLAUDE.md", "AGENTS.md", ".claude/rules", ".claude/agents"];

#[derive(Default)]
pub(super) struct MachinePlan {
    pub present: Vec<Spec>,
    pub missing: Vec<Spec>,
}

fn spec(name: &str, kind: SourceKind, root: std::path::PathBuf) -> Spec {
    Spec {
        name: name.to_owned(),
        kind,
        root,
        scope: Scope::default(),
        declared: false,
    }
}

pub(super) fn machine_specs(claude_home: &Path) -> [Spec; 4] {
    let mut rules = spec(
        RULES_SOURCE,
        SourceKind::Instructions,
        claude_home.join("rules"),
    );
    rules.scope.extras = vec![claude_home.join(GLOBAL_INSTRUCTIONS_FILE)];
    [
        spec(
            AGENTS_SOURCE,
            SourceKind::Instructions,
            claude_home.join("agents"),
        ),
        rules,
        spec(
            SKILLS_SOURCE,
            SourceKind::Skills,
            claude_home.join("skills"),
        ),
        spec(
            AGENT_MEMORY_SOURCE,
            SourceKind::Memory,
            claude_home.join("agent-memory"),
        ),
    ]
}

pub(super) async fn plan(claude_home: &Path) -> MachinePlan {
    let mut plan = MachinePlan::default();
    for spec in machine_specs(claude_home) {
        let present = tokio::fs::metadata(&spec.root)
            .await
            .is_ok_and(|meta| meta.is_dir());
        if present {
            plan.present.push(spec);
        } else {
            plan.missing.push(spec);
        }
    }
    plan
}

pub(super) fn inputs<'a>(
    machine: Uuid,
    specs: impl Iterator<Item = &'a Spec>,
) -> Vec<MachineSourceInput> {
    specs
        .map(|spec| MachineSourceInput {
            machine_uuid: machine,
            name: spec.name.clone(),
            root_path: spec.root.to_string_lossy().into_owned(),
        })
        .collect()
}

pub(super) async fn record_missing(
    persist: &PersistHandle,
    machine: Uuid,
    missing: &[Spec],
    memo: &mut ScanMemo,
) -> Result<usize, PersistError> {
    if missing.is_empty() {
        return Ok(0);
    }
    for spec in missing {
        memo.forget(machine, &spec.name);
        tracing::debug!(root = %spec.root.display(), source = %spec.name,
            "knowledge_scan: machine source root does not exist; the earlier index is kept");
    }
    let recorded = persist
        .knowledge()
        .record_machine_knowledge_roots_missing(&inputs(machine, missing.iter()))
        .await?;
    Ok(usize::try_from(recorded).unwrap_or(usize::MAX))
}
