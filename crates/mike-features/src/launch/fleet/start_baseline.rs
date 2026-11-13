use crate::store::persistence::fleet_repo::FleetPersist as _;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::definition::FleetDefinition;
use super::plan::FleetFacts;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::FleetRow;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Baseline {
    pub commit: String,
    pub command: String,
    pub result_id: i64,
    pub passed: bool,
    pub verified: bool,
    pub finished_at: Option<DateTime<Utc>>,
    pub repair_task: Option<String>,
}

pub fn repair_task(def: &FleetDefinition) -> Result<Option<&str>, String> {
    let Some(id) = def.baseline.as_ref().and_then(|b| b.repair_task.as_deref()) else {
        return Ok(None);
    };
    let task = def
        .tasks
        .get(id)
        .ok_or_else(|| format!("baseline.repair_task `{id}` must name an existing task"))?;
    if !task.after.is_empty() || !def.workers.contains_key(&task.discipline) {
        return Err(format!(
            "baseline.repair_task `{id}` needs a declared worker discipline and no dependencies"
        ));
    }
    Ok(Some(id))
}

async fn git(repo: &str, args: &[&str]) -> Result<String, String> {
    super::watch::git(repo, args, None)
        .await
        .map(|raw| String::from_utf8_lossy(&raw).trim().to_string())
        .ok_or_else(|| format!("baseline git {args:?} failed"))
}

pub async fn clean_commit(def: &FleetDefinition) -> Result<String, String> {
    let commit = git(&def.repo, &["rev-parse", "--verify", "HEAD"]).await?;
    let branch = super::check::head(def).await?;
    if branch.commit != commit {
        return Err("baseline checkout HEAD does not match the fleet branch".to_string());
    }
    let changes = git(
        &def.repo,
        &["status", "--porcelain", "--untracked-files=normal"],
    )
    .await?;
    if !changes.is_empty() {
        return Err(format!(
            "baseline checkout has changes; preserve and settle them before starting:\n{changes}"
        ));
    }
    Ok(commit)
}

pub async fn establish(
    persist: &PersistHandle,
    def: &mut FleetDefinition,
    facts: &FleetFacts,
    tracker: &tokio_util::task::TaskTracker,
) -> Result<(), String> {
    repair_task(def)?;
    if !def.gate.declared() {
        return Ok(());
    }
    super::gate_slot::prepare(&def.repo).await?;
    let _held = startup_lock(&def.repo).await?;
    let slot = super::gate_slot::slot_for(&def.repo)
        .ok_or("startup baseline needs a repository checkout")?;
    if super::gate::running_commit(&slot.to_string_lossy()).is_some() {
        return Err(
            "a gate is already running in the startup checkout; retry after it finishes"
                .to_string(),
        );
    }
    let commit = clean_commit(def).await?;
    let ledger = super::gate::ledger_for(def, &facts.run_id);
    let managed =
        super::managed_command::begin_baseline(persist, def, facts, &commit, tracker).await?;
    let id = managed.id;
    let owner = managed.context.owner.clone();
    let target = super::gate::Target {
        commit: commit.clone(),
        pushed: false,
        slot,
    };
    let (mut code, mut output, isolated) = super::managed_command::scope(
        managed.context,
        super::gate::execute_verified(def, &target, &facts.run_id),
    )
    .await;
    let verified = source_unchanged(def, &commit, isolated && code.is_some(), &mut output).await;
    let cancelled = super::managed_command::normalize_cancellation(&owner, &mut code, &mut output);
    let verified = verified && !cancelled;
    let code = verified.then_some(code).flatten();
    let after = if verified {
        commit.clone()
    } else {
        format!("{commit}:unverified")
    };
    owner.phase(super::owned_execution::Phase::RecordingReceipt, None);
    let captured = super::check_run::completion::Captured {
        id,
        exit_code: code,
        output: output.clone(),
        state_after: if owner.cancelled().is_some() {
            "cancelled:unverified".into()
        } else {
            after
        },
    };
    match super::check_run::completion::record(persist, tracker, &ledger, &def.repo, captured).await
    {
        Ok(_) => managed.registration.receipt_recorded(),
        Err(why) => {
            managed.registration.receipt_blocked(why.clone());
            return Err(why);
        }
    }
    let baseline = Baseline {
        commit,
        command: def.gate.command.clone(),
        result_id: id,
        passed: code == Some(0),
        verified,
        finished_at: Some(Utc::now()),
        repair_task: def.baseline.as_ref().and_then(|b| b.repair_task.clone()),
    };
    if !super::seat::patch_definition(
        persist,
        facts
            .fleet
            .parse()
            .map_err(|e: uuid::Error| e.to_string())?,
        &json!({"baseline": baseline}),
    )
    .await
    {
        return Err("the startup baseline result could not be attached to the fleet".to_string());
    }
    def.baseline = Some(baseline);
    if !verified {
        return Err(format!(
            "startup gate could not establish a verified baseline; full output is in tree-check result {id}:\n{output}"
        ));
    }
    hold_workers_when_red(def, code, id, &output, &ledger)
}

fn hold_workers_when_red(
    def: &mut FleetDefinition,
    code: Option<i32>,
    id: i64,
    output: &str,
    ledger: &impl std::fmt::Display,
) -> Result<(), String> {
    if let Some(code @ (126 | 127)) = code {
        return Err(format!(
            "the gate command could not run in the clean checkout (exit {code}: a program it calls is missing or not executable); fix `{}` so it installs what it needs, then send ready again. Output is in tree-check result {id}:\n{output}",
            def.gate.command
        ));
    }
    if code != Some(0) && repair_task(def).ok().flatten().is_none() {
        return Err(red_without_repair(&def.gate.command, id, ledger));
    }
    if code != Some(0) {
        def.coordinator.brief.push_str(&format!(
            "\nSTARTUP BASELINE IS RED (result {id}). Ordinary workers are held. Define a properly scoped, dependency-free repair task with a declared worker discipline and set baseline.repair_task to its ID. Only that task can start until the current clean commit passes `{}`. Full failure output is retained in the gate tree-check ledger {}.",
            def.gate.command, ledger,
        ));
    }
    Ok(())
}

async fn source_unchanged(
    def: &FleetDefinition,
    commit: &str,
    isolated: bool,
    output: &mut String,
) -> bool {
    match clean_commit(def).await {
        Ok(after) if after == commit => isolated,
        Ok(_) => {
            output.push_str("\nBaseline checkout changed commits during the gate.");
            false
        }
        Err(why) => {
            output.push_str(&format!("\nBaseline result is unverified: {why}"));
            false
        }
    }
}

async fn startup_lock(repo: &str) -> Result<std::fs::File, String> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .read(true)
        .open(std::path::Path::new(repo).join(".claude/worktrees/start-baseline.lock"))
        .map_err(|e| e.to_string())?;
    file.try_lock()
        .map_err(|e| format!("another startup baseline owns this checkout: {e}"))?;
    Ok(file)
}

pub fn red_without_repair(
    gate: &str,
    result: impl std::fmt::Display,
    ledger: &impl std::fmt::Display,
) -> String {
    format!(
        "the gate `{gate}` fails on the branch's clean commit (tree-check result {result}, ledger {ledger}), and a red baseline starts only baseline.repair_task, which the definition does not name, so no worker could start. Add a dependency-free task that makes the gate pass, set baseline.repair_task to it, and send ready again"
    )
}

pub fn blocked(def: &FleetDefinition) -> bool {
    def.baseline
        .as_ref()
        .is_some_and(|b| !b.verified || !b.passed || b.command != def.gate.command)
}

pub fn admitted(def: &FleetDefinition, ready: Vec<String>, passed: bool) -> Vec<String> {
    if !blocked(def) || passed {
        return ready;
    }
    let Some(repair) = repair_task(def).ok().flatten() else {
        return Vec::new();
    };
    let repair_files = task_files(def, repair);
    ready
        .into_iter()
        .filter(|id| id == repair || clear_of(&task_files(def, id), &repair_files))
        .collect()
}

fn task_files(def: &FleetDefinition, id: &str) -> Vec<String> {
    def.tasks.get(id).map_or_else(Vec::new, |task| {
        task.files
            .iter()
            .chain(&task.new_files)
            .map(|file| super::checks::normal(file))
            .filter(|file| !file.is_empty())
            .collect()
    })
}

fn clear_of(files: &[String], repair_files: &[String]) -> bool {
    !files.is_empty()
        && !repair_files.is_empty()
        && files.iter().all(|file| {
            repair_files
                .iter()
                .all(|repair| !super::checks::overlaps(file, repair))
        })
}

pub async fn staffable(
    persist: &PersistHandle,
    row: &FleetRow,
    def: &FleetDefinition,
    ready: Vec<String>,
) -> Vec<String> {
    if !blocked(def) {
        return ready;
    }
    let passed = current_pass(persist, row, def).await;
    admitted(def, ready, passed)
}

async fn current_pass(persist: &PersistHandle, row: &FleetRow, def: &FleetDefinition) -> bool {
    let Some(run) = row.run_id.as_deref() else {
        return false;
    };
    let Ok(commit) = clean_commit(def).await else {
        return false;
    };
    let Ok(Some(result)) = persist
        .fleet()
        .find_tree_check(&super::gate::ledger_for(def, run), &commit)
        .await
    else {
        return false;
    };
    result.passed()
        && result.finished_at.is_some()
        && !result.moved()
        && result.command == def.gate.command
}

#[cfg(test)]
#[path = "start_baseline_tests.rs"]
pub mod tests;
