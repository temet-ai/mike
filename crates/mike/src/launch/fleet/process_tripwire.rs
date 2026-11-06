use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::tell::Notice;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Setting {
    Check,
    Gate,
}

impl Setting {
    fn command(self, def: &FleetDefinition) -> &str {
        match self {
            Self::Check => &def.check.command,
            Self::Gate => &def.gate.command,
        }
    }

    fn path(self) -> &'static str {
        match self {
            Self::Check => "check.command",
            Self::Gate => "gate.command",
        }
    }
}

#[derive(Debug, Clone)]
struct Execution {
    id: Uuid,
    run: String,
    repo: PathBuf,
    command: String,
    setting: Setting,
    pid: u32,
    started_at: DateTime<Utc>,
}

fn executions() -> &'static DashMap<Uuid, Execution> {
    static EXECUTIONS: OnceLock<DashMap<Uuid, Execution>> = OnceLock::new();
    EXECUTIONS.get_or_init(DashMap::new)
}

pub(super) struct OwnedExecution(Uuid);

impl Drop for OwnedExecution {
    fn drop(&mut self) {
        executions().remove(&self.0);
    }
}

pub(super) fn register(
    run: &str,
    repo: &Path,
    command: &str,
    setting: Option<Setting>,
    pid: u32,
) -> Option<OwnedExecution> {
    let setting = setting?;
    let id = Uuid::new_v4();
    executions().insert(
        id,
        Execution {
            id,
            run: run.to_string(),
            repo: canonical(repo),
            command: command.to_string(),
            setting,
            pid,
            started_at: Utc::now(),
        },
    );
    Some(OwnedExecution(id))
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn changed(execution: &Execution, def: &FleetDefinition) -> bool {
    execution.command.trim() != execution.setting.command(def).trim()
}

pub(super) fn notices(run: &str, def: &FleetDefinition, coordinator: Uuid) -> Vec<Notice> {
    executions()
        .iter()
        .filter(|execution| execution.run == run && changed(execution, def))
        .map(|execution| notice(&execution, def, coordinator))
        .collect()
}

fn notice(execution: &Execution, def: &FleetDefinition, coordinator: Uuid) -> Notice {
    Notice {
        to: coordinator,
        key: format!("process-setting:{}", execution.id),
        covers: Vec::new(),
        body: format!(
            "[mike tripwire] Mike's tracked process {} for {} has not ended since starting at {}. Its declared command has changed.\nStarted command: {}\nExecution checkout: {}\nCurrent {}: {}\nFleet source checkout: {}\nDecide whether this execution should finish or be cancelled. This notice is based on Mike's owned subprocess lifecycle, not its output age; no hang or failure is inferred and nothing was killed.",
            execution.pid,
            execution.setting.path(),
            execution.started_at.to_rfc3339(),
            execution.command,
            execution.repo.display(),
            execution.setting.path(),
            execution.setting.command(def),
            def.repo,
        ),
    }
}

#[cfg(test)]
#[path = "process_tripwire_tests.rs"]
mod tests;
