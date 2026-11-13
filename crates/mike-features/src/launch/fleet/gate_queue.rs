use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::path::Path;

use uuid::Uuid;

use super::check::Runner;
use super::check_run;
use super::definition::FleetDefinition;
use super::gate::{self, Target};
use super::gate_failures::{inventory, summary};
use super::gate_rerun::{
    Rerun, cleared, only_timeouts_that_pass, recorded_reruns, rerun_lines, rerun_timeouts,
};
use super::gate_slot::slot_for;
use super::plan::ControlSeats;
use super::watch::git;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::FleetRow;

pub use super::gate_blame::{Blame, Suspect, blame};

const AUTOMATIC_SEAT: &str = "mike";
const LOOK_BACK: &str = "200";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Start,
    Checked,
    Running,
}

pub const RETRY_AFTER: chrono::Duration = chrono::Duration::minutes(10);

pub fn backing_off(
    latest: Option<&crate::store::persistence::fleet::TreeCheckRow>,
    commit: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<&'static str> {
    let row = latest?;
    let ended_without_result = row.exit_code.is_none()
        && row
            .finished_at
            .is_some_and(|finished| now - finished < RETRY_AFTER);
    (row.state == commit && ended_without_result).then_some(
        "the last automatic gate at this commit ended without a result less than 10 minutes ago",
    )
}

pub fn decide(recorded: bool, running: Option<&str>) -> Decision {
    if running.is_some() {
        Decision::Running
    } else if recorded {
        Decision::Checked
    } else {
        Decision::Start
    }
}

pub fn unchecked_from(commit: &str, passed: Option<bool>) -> Option<String> {
    (passed != Some(true)).then(|| commit.to_string())
}

pub fn said(raw: Option<Vec<u8>>) -> Option<String> {
    raw.map(|bytes| String::from_utf8_lossy(&bytes).trim().to_string())
        .filter(|text| !text.is_empty())
}

pub async fn commit_to_check(def: &FleetDefinition) -> Option<Target> {
    let origin = format!("refs/remotes/origin/{}", def.branch);
    let local = format!("refs/heads/{}", def.branch);
    let (commit, pushed) = match said(git(&def.repo, &["rev-parse", &origin], None).await) {
        Some(commit) => (commit, true),
        None => (
            said(git(&def.repo, &["rev-parse", &local], None).await)?,
            false,
        ),
    };
    Some(Target {
        commit,
        pushed,
        slot: slot_for(&def.repo)?,
    })
}

pub async fn queue(state: &AppState, persist: &PersistHandle, row: &FleetRow) {
    let (Some(run), Ok(def)) = (
        row.run_id.as_deref(),
        FleetDefinition::from_value(&row.definition),
    ) else {
        return;
    };
    let runner = Runner {
        persist,
        tracker: &state.tracker,
        run,
        fleet: row.fleet_uuid,
    };
    check_run::queue(&runner, &def).await;
    queue_gate(&runner, &def).await;
}

async fn queue_gate(runner: &Runner<'_>, def: &FleetDefinition) {
    if !def.gate.declared() {
        return;
    }
    let Some(target) = commit_to_check(def).await else {
        return;
    };
    queue_target(runner, def, &target).await;
}

async fn queue_target(runner: &Runner<'_>, def: &FleetDefinition, target: &Target) {
    let _held = gate::admission::hold(&target.slot).await;
    if !should_start(runner, def, target).await {
        return;
    }
    if let Err(err) = gate::start(runner, def, AUTOMATIC_SEAT, target).await {
        tracing::warn!(run = runner.run, error = %err, "fleet: automatic gate was not queued");
    }
}

async fn should_start(runner: &Runner<'_>, def: &FleetDefinition, target: &Target) -> bool {
    let ledger = gate::ledger_for(def, runner.run);
    let recorded = matches!(
        runner.persist.fleet().find_tree_check(&ledger, &target.commit).await,
        Ok(Some(row)) if row.command == def.gate.command
    );
    let running = gate::running_commit(&target.slot.to_string_lossy());
    if decide(recorded, running.as_deref()) != Decision::Start {
        return false;
    }
    let latest = runner
        .persist
        .fleet()
        .latest_tree_check(&ledger)
        .await
        .ok()
        .flatten();
    match backing_off(latest.as_ref(), &target.commit, chrono::Utc::now()) {
        Some(why) => {
            tracing::debug!(run = runner.run, commit = %target.commit, why, "fleet: automatic gate held back");
            false
        }
        None => true,
    }
}

pub async fn unchecked(persist: &PersistHandle, row: &FleetRow) -> Option<String> {
    let run = row.run_id.as_deref()?;
    let def = FleetDefinition::from_value(&row.definition).ok()?;
    if !def.gate.declared() {
        return None;
    }
    let target = commit_to_check(&def).await?;
    let passed = commit_passed(persist, &def, run, &target.commit).await?;
    unchecked_from(&target.commit, passed)
}

async fn commit_passed(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run: &str,
    commit: &str,
) -> Option<Option<bool>> {
    let check = persist
        .fleet()
        .find_tree_check(&gate::ledger_for(def, run), commit)
        .await
        .ok()?;
    let Some(check) = check else {
        return Some(None);
    };
    if check.command != def.gate.command {
        return Some(None);
    }
    if check.passed() {
        return Some(Some(true));
    }
    let reruns = recorded_reruns(persist, &def.gate, (run, commit), &check.output).await;
    Some(Some(cleared(&def.gate, &check.output, &reruns)))
}

pub async fn commits_since(repo: &str, passed: Option<&str>, failed: &str) -> Vec<String> {
    let range = passed.map_or_else(|| failed.to_string(), |p| format!("{p}..{failed}"));
    let args = ["rev-list", "--reverse", "-n", LOOK_BACK, &range];
    said(git(repo, &args, None).await)
        .map(|list| list.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

fn short(commit: &str) -> &str {
    &commit[..commit.len().min(10)]
}

pub fn seat_mail(
    command: &str,
    failed: &str,
    suspect: &Suspect,
    reruns: &[Rerun],
    output: &str,
    log: &Path,
) -> String {
    let mut lines = vec![format!(
        "[mike gate] `{command}` failed at commit {}. Your commit {} {}; {} commit(s) were checked together. Fix it and push; mike runs the gate again on the newest pushed commit by itself.",
        short(failed),
        short(&suspect.commit),
        suspect.evidence.reason(),
        suspect.since,
    )];
    lines.extend(rerun_lines(reruns));
    lines.push(summary(&inventory(output), &log.to_string_lossy()));
    lines.join("\n")
}

pub fn pre_existing_mail(
    command: &str,
    failed: &str,
    reruns: &[Rerun],
    output: &str,
    log: &Path,
) -> String {
    let mut lines = vec![format!(
        "[mike gate] PRE-EXISTING failure: `{command}` failed at commit {}, and no seat of this run has committed since the last passing commit, so the failure was already there when the run began. Add a task for it now instead of finding it at the end; fail-fast may hide more past the first failure.",
        short(failed),
    )];
    lines.extend(rerun_lines(reruns));
    lines.push(summary(&inventory(output), &log.to_string_lossy()));
    lines.join("\n")
}

pub fn one_line(command: &str, failed: &str, blame: &Blame, reruns: &[Rerun]) -> String {
    let timeouts = if only_timeouts_that_pass(reruns) {
        format!("; {} timed-out test(s) passed alone", reruns.len())
    } else {
        String::new()
    };
    match blame {
        Blame::Suspects(suspects) => {
            let named: Vec<String> = suspects
                .iter()
                .map(|suspect| format!("{} by {}", short(&suspect.commit), suspect.seat))
                .collect();
            let (first, also) = named.split_first().map_or(("", &[][..]), |(f, a)| (f, a));
            let also = if also.is_empty() {
                String::new()
            } else {
                format!("; also {}", also.join(", "))
            };
            format!(
                "[mike gate] `{command}` failed at commit {}; first suspect: {first}{also}{timeouts}",
                short(failed)
            )
        }
        Blame::NoMatch { since } => format!(
            "[mike gate] `{command}` failed at commit {}; none of the {since} commit(s) since the last pass adds a name or touches a file the failing output names, so no seat is named and the verdict is yours{timeouts}",
            short(failed)
        ),
    }
}

fn suspect_sessions<'a>(def: &FleetDefinition, blame: &'a Blame) -> Vec<(Uuid, &'a Suspect)> {
    let Blame::Suspects(suspects) = blame else {
        return Vec::new();
    };
    let mut sessions: Vec<(Uuid, &Suspect)> = Vec::new();
    for suspect in suspects {
        if let Some(session) = session_of_seat(def, &suspect.seat)
            && !sessions.iter().any(|(seen, _)| *seen == session)
        {
            sessions.push((session, suspect));
        }
    }
    sessions
}

fn session_of_seat(def: &FleetDefinition, seat: &str) -> Option<Uuid> {
    def.tasks
        .values()
        .find(|task| task.seat.as_deref() == Some(seat))
        .and_then(|task| task.session.as_deref())
        .and_then(|session| session.trim().parse().ok())
}

async fn mail(persist: &PersistHandle, to: Uuid, body: &str) {
    match persist
        .launch()
        .insert_message(Uuid::new_v4(), to, None, body)
        .await
    {
        Ok(Some(_)) => {}
        Ok(None) => missing_recipient(to),
        Err(err) => failed_mail(to, &err),
    }
}

fn missing_recipient(to: Uuid) {
    tracing::warn!(%to, "fleet: gate recipient is missing or closed");
}

fn failed_mail(to: Uuid, err: &crate::store::persistence::PersistError) {
    tracing::warn!(%to, error = %err, "fleet: could not mail the gate's verdict");
}

async fn last_passing(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run: &str,
    failed: &str,
) -> Option<String> {
    let before = commits_since(&def.repo, None, failed).await;
    for commit in before.iter().rev().skip(1) {
        if commit_passed(persist, def, run, commit).await == Some(Some(true)) {
            return Some(commit.clone());
        }
    }
    None
}

pub async fn after_run(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run: &str,
    target: &Target,
    (code, output, log, verified): (Option<i32>, &str, &Path, bool),
) {
    super::check_report::record_finished(
        persist,
        def,
        super::check_report::Finished {
            label: "gate",
            run,
            command: &def.gate.command,
            commit: &target.commit,
            passed: code == Some(0),
            source_verified: verified,
            output,
            root: Some(target.slot.to_string_lossy().into_owned()),
        },
    )
    .await;
    tell_acceptance(persist, def, run, target, (code, output, log, verified)).await;
    if code == Some(0) || !verified {
        return;
    }
    let owners =
        super::gate_failures::tell_owners(persist, def, run, target, (code, output, log)).await;
    let reruns = rerun_timeouts(persist, def, run, target, output).await;
    let passed = last_passing(persist, def, run, &target.commit).await;
    let command = def.gate.command.as_str();
    let Some(blame) = blame(def, passed.as_deref(), &target.commit, output).await else {
        if !only_timeouts_that_pass(&reruns)
            && let Some(seats) = ControlSeats::recorded(def)
        {
            let body = pre_existing_mail(command, &target.commit, &reruns, output, log);
            mail(persist, seats.coordinator, &body).await;
        }
        return;
    };
    let line = one_line(command, &target.commit, &blame, &reruns);
    let suspects = suspect_sessions(def, &blame);
    for (session, suspect) in &suspects {
        if !owners.contains(session) && !only_timeouts_that_pass(&reruns) {
            let body = seat_mail(command, &target.commit, suspect, &reruns, output, log);
            mail(persist, *session, &body).await;
        }
    }
    if let Some(seats) = ControlSeats::recorded(def)
        && !suspects
            .iter()
            .any(|(session, _)| *session == seats.coordinator)
    {
        mail(persist, seats.coordinator, &line).await;
    }
}

pub fn completion_mail(
    command: &str,
    commit: &str,
    code: Option<i32>,
    output: &str,
    log: &Path,
) -> String {
    let verdict = match code {
        Some(0) => "green".to_owned(),
        Some(code) => format!("failed (exit {code})"),
        None => "not completed (no exit code)".to_owned(),
    };
    let detail = if code == Some(0) {
        String::new()
    } else {
        format!("\n{}", summary(&inventory(output), &log.to_string_lossy()))
    };
    format!(
        "[mike gate] `{command}` at commit {commit}: {verdict}. Log: {}{detail}",
        log.display()
    )
}

async fn tell_acceptance(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run: &str,
    target: &Target,
    (code, output, log, verified): (Option<i32>, &str, &Path, bool),
) {
    let Some(initial) = super::plan::ControlSeats::recorded(def).map(|c| c.acceptance) else {
        return;
    };
    let current = persist
        .fleet()
        .find_fleet_for_session(initial)
        .await
        .ok()
        .flatten()
        .filter(|row| row.run_id.as_deref() == Some(run))
        .and_then(|row| FleetDefinition::from_value(&row.definition).ok())
        .and_then(|def| def.acceptance.session)
        .and_then(|s| s.parse::<Uuid>().ok())
        .unwrap_or(initial);
    let mut body = completion_mail(&def.gate.command, &target.commit, code, output, log);
    if !verified {
        body.push_str(
            "\nSource unverified: this execution provides no verdict for the named commit.",
        );
    }
    if !log.is_file() {
        body.push_str("\nThe log is unavailable; retrieve the full output from the persisted gate result for this command and commit.");
    }
    mail(persist, current, &body).await;
}

pub fn completion_log(
    target: &Target,
    command: &str,
    execution: &str,
) -> std::path::PathBuf {
    use sha2::{Digest, Sha256};
    let command_hash = hex::encode(Sha256::digest(command.as_bytes()));
    target.slot.join(format!(
        ".claude/worktrees/mike-gate-{}-{command_hash}-{execution}-output.log",
        target.commit
    ))
}

#[cfg(test)]
#[path = "gate_completion_tests.rs"]
pub mod completion_tests;
