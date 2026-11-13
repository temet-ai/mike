use crate::store::persistence::fleet_repo::FleetPersist as _;
use std::borrow::Cow;
use std::time::Duration;

use tokio_util::task::TaskTracker;

use crate::launch::fleet::check::Runner;
use crate::launch::fleet::checks::Missing;
use crate::launch::fleet::definition::FleetDefinition;
use crate::launch::fleet::gate_queue::{backing_off, commit_to_check};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::FleetRow;

pub const LEDGER_SUFFIX: &str = ":full";
const CLOSE_SEAT: &str = "mike";
const FULL_WORD: &str = "full";
const LONG_GATE: Duration = Duration::from_secs(10 * 60);

pub fn separate(def: &FleetDefinition) -> bool {
    let full = def.gate.full_command.trim();
    !full.is_empty() && full != def.gate.command.trim()
}

pub(in crate::launch::fleet) fn is_full(def: &FleetDefinition) -> bool {
    let full = def.gate.full_command.trim();
    !full.is_empty() && full == def.gate.command.trim()
}

pub fn command(def: &FleetDefinition) -> &str {
    if separate(def) {
        &def.gate.full_command
    } else {
        &def.gate.command
    }
}

pub fn definition(def: &FleetDefinition) -> Cow<'_, FleetDefinition> {
    if !separate(def) {
        return Cow::Borrowed(def);
    }
    let mut full = def.clone();
    full.gate.command = def.gate.full_command.trim().to_string();
    full.gate.full_command = full.gate.command.clone();
    Cow::Owned(full)
}

pub fn warnings(def: &FleetDefinition) -> Vec<Missing> {
    if !def.gate.declared() || separate(def) {
        return Vec::new();
    }
    let measured = crate::launch::fleet::managed_queue::measured(&def.gate.command);
    full_gate_reason(def, measured)
        .map(|why| {
            Missing::new(
                "gate.command",
                format!(
                    "`{}` looks like the project's full gate ({why}); mike runs gate.command before any seat opens and on every pushed commit, so set gate.command to a quick format-and-compile gate and put the full gate in gate.full_command, which runs once when the work is done",
                    def.gate.command.trim()
                ),
            )
        })
        .into_iter()
        .collect()
}

pub(in crate::launch::fleet) fn full_gate_reason(
    def: &FleetDefinition,
    measured: Option<Duration>,
) -> Option<String> {
    let command = def.gate.command.trim();
    if let Some((name, op)) = def.execution.named(command) {
        let said = format!("{name} {}", op.purpose).to_lowercase();
        if said
            .split(|c: char| !c.is_alphanumeric())
            .any(|w| w == FULL_WORD)
        {
            return Some(format!(
                "its declared operation `{name}` says: {}",
                op.purpose
            ));
        }
        if op
            .timeout_seconds
            .is_some_and(|secs| secs > LONG_GATE.as_secs())
        {
            return Some(format!(
                "its declared operation `{name}` allows {} minutes",
                op.timeout_seconds.unwrap_or_default() / 60
            ));
        }
    }
    measured
        .filter(|took| *took > LONG_GATE)
        .map(|took| format!("a recorded run took {} minutes", took.as_secs() / 60))
}

pub(in crate::launch::fleet) async fn unchecked_at_close(
    tracker: &TaskTracker,
    persist: &PersistHandle,
    row: &FleetRow,
) -> Option<String> {
    if let Some(commit) = crate::launch::fleet::gate_queue::unchecked(persist, row).await {
        return Some(commit);
    }
    let run = row.run_id.as_deref()?;
    let def = FleetDefinition::from_value(&row.definition).ok()?;
    if !def.gate.declared() || !separate(&def) {
        return None;
    }
    let full = definition(&def);
    let target = commit_to_check(&full).await?;
    let runner = Runner {
        persist,
        tracker,
        run,
        fleet: row.fleet_uuid,
    };
    match close_verdict(&runner, &full, &target.commit).await {
        Some(true) => None,
        Some(false) => Some(target.commit),
        None => {
            start_once(&runner, &full, &target).await;
            Some(target.commit)
        }
    }
}

async fn close_verdict(runner: &Runner<'_>, full: &FleetDefinition, commit: &str) -> Option<bool> {
    let ledger = super::ledger_for(full, runner.run);
    match runner
        .persist
        .fleet()
        .find_tree_check(&ledger, commit)
        .await
    {
        Ok(Some(row)) if row.command == full.gate.command => Some(row.passed()),
        Ok(_) => None,
        Err(err) => {
            tracing::warn!(run = runner.run, error = %err, "fleet: could not read the full gate's result at close");
            Some(false)
        }
    }
}

#[cfg(test)]
#[path = "gate_full_tests.rs"]
mod tests;

async fn start_once(runner: &Runner<'_>, full: &FleetDefinition, target: &super::Target) {
    let _held = super::admission::hold(&target.slot).await;
    if super::running_commit(&target.slot.to_string_lossy()).is_some() {
        return;
    }
    if held_back(runner, full, &target.commit).await {
        return;
    }
    match super::start(runner, full, CLOSE_SEAT, target).await {
        Ok(_) => log_started(runner.run, &target.commit, &full.gate.command),
        Err(err) => log_not_started(runner.run, &err),
    }
}

async fn held_back(runner: &Runner<'_>, full: &FleetDefinition, commit: &str) -> bool {
    let ledger = super::ledger_for(full, runner.run);
    let latest = runner
        .persist
        .fleet()
        .latest_tree_check(&ledger)
        .await
        .ok()
        .flatten();
    let Some(why) = backing_off(latest.as_ref(), commit, chrono::Utc::now()) else {
        return false;
    };
    tracing::debug!(run = runner.run, %commit, why, "fleet: full gate at close held back");
    true
}

fn log_started(run: &str, commit: &str, command: &str) {
    tracing::info!(run, %commit, %command, "fleet: every seat has exited; the full gate runs once at the branch's newest commit");
}

fn log_not_started(run: &str, err: &str) {
    tracing::warn!(run, error = %err, "fleet: the full gate at close was not started");
}
