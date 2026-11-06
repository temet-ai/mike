use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use uuid::Uuid;

use super::check::{Runner, asker};
use super::check_run::completion;
use super::definition::FleetDefinition;
use super::gate_failures::inventory;
use super::gate_queue::completion_log;
use super::gate_slot::{OUTPUT_FILE, slot_for};
use super::gate_text::text;
use super::seated::Seated;
use super::watch::git;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::TreeCheckRow;

const TAIL_LINES: usize = 60;
const ACCEPTANCE: &str = "acceptance";
pub const DEFAULT_WAIT_SECONDS: u64 = 45;
pub const LEDGER_SUFFIX: &str = "#gate:isolated-source-v2";
pub const RUNNING: &str = "running";
pub const NOT_RUN: &str = "not run";
const REUSED: &str = "reused";
const FRESH: &str = "fresh";
const JOINED: &str = "joined";

#[path = "gate_admission.rs"]
pub(super) mod admission;
#[path = "gate_execution.rs"]
mod execution;
pub(super) use execution::{execute_verified, start};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateRequest {
    pub session: Uuid,
    #[serde(default)]
    pub wait_seconds: Option<u64>,
    #[serde(default)]
    pub status: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateReport {
    pub seat: String,
    pub command: String,
    pub commit: String,
    pub pushed: bool,
    pub how: String,
    pub passed: bool,
    pub exit_code: Option<i32>,
    pub seconds: i64,
    pub checkout: String,
    pub output_file: String,
    pub output_tail: String,
    #[serde(default)]
    pub failure_inventory: Vec<String>,
    #[serde(default)]
    pub source_verified: bool,
    pub refused: Option<String>,
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
}

struct InFlight {
    ledger: String,
    target: Target,
    command: String,
    started_at: DateTime<Utc>,
    done: watch::Receiver<Option<TreeCheckRow>>,
}

fn in_flight() -> &'static DashMap<String, InFlight> {
    static IN_FLIGHT: OnceLock<DashMap<String, InFlight>> = OnceLock::new();
    IN_FLIGHT.get_or_init(DashMap::new)
}

fn waiting() -> &'static DashMap<String, Vec<Uuid>> {
    static WAITING: OnceLock<DashMap<String, Vec<Uuid>>> = OnceLock::new();
    WAITING.get_or_init(DashMap::new)
}

pub fn ledger(run: &str) -> String {
    format!("{run}{LEDGER_SUFFIX}")
}

pub fn ledger_for(def: &FleetDefinition, run: &str) -> String {
    let operation = def.execution.operation(&def.gate.command);
    if operation.is_none() && !def.execution.cargo {
        return ledger(run);
    }
    let policy = serde_json::json!([def.execution.cargo, operation]).to_string();
    let identity = super::check_state::command_state(&policy, &def.gate.command);
    format!("{}:{identity}", ledger(run))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Target {
    pub(super) commit: String,
    pub(super) pushed: bool,
    pub(super) slot: PathBuf,
}

async fn target(def: &FleetDefinition) -> Result<Target, String> {
    let said = |raw: Option<Vec<u8>>| {
        raw.map(|bytes| String::from_utf8_lossy(&bytes).trim().to_string())
            .filter(|text| !text.is_empty())
    };
    let head = format!("refs/heads/{}", def.branch);
    let commit = said(git(&def.repo, &["rev-parse", &head], None).await).ok_or_else(|| {
        format!(
            "the branch `{}` has no commit to run the gate at",
            def.branch
        )
    })?;
    let origin = format!("refs/remotes/origin/{}", def.branch);
    let pushed =
        said(git(&def.repo, &["rev-parse", &origin], None).await).as_ref() == Some(&commit);
    let slot = slot_for(&def.repo)
        .ok_or_else(|| format!("`{}` is not a checkout to run the gate in", def.repo))?;
    Ok(Target {
        commit,
        pushed,
        slot,
    })
}

pub async fn gate(
    runner: Runner<'_>,
    def: &FleetDefinition,
    seated: &Seated,
    request: &GateRequest,
) -> GateReport {
    let seat = asker(def, seated, request.session).map(|who| who.seat);
    let Some(seat) = seat else {
        return refused(
            "?",
            def,
            format!("session {} holds no seat in this fleet", request.session),
        );
    };
    if let Some(problem) = super::operation::validate(def).first() {
        return refused(&seat, def, format!("{}: {}", problem.item, problem.problem));
    }
    if !def.gate.declared() {
        return refused(
            &seat,
            def,
            "this fleet declares no gate command (definition `gate.command`)".to_string(),
        );
    }
    let target = match target(def).await {
        Ok(target) => target,
        Err(why) => return refused(&seat, def, why),
    };
    match obtain(&runner, def, (&seat, request), &target).await {
        Ok(Outcome::Finished(row, how)) => finished(&seat, def, &target, &row, how),
        Ok(Outcome::Running {
            target,
            command,
            started_at,
            how,
        }) => running_report(&seat, def, &target, command, started_at, how),
        Ok(Outcome::NotRun) => GateReport {
            how: NOT_RUN.to_string(),
            ..blank(&seat, def, &target)
        },
        Err(why) => GateReport {
            refused: Some(why),
            how: "refused".to_string(),
            ..blank(&seat, def, &target)
        },
    }
}

enum Outcome {
    Finished(Box<TreeCheckRow>, &'static str),
    Running {
        target: Target,
        command: String,
        started_at: DateTime<Utc>,
        how: &'static str,
    },
    NotRun,
}

fn blank(seat: &str, def: &FleetDefinition, target: &Target) -> GateReport {
    GateReport {
        seat: seat.to_string(),
        command: def.gate.command.clone(),
        commit: target.commit.clone(),
        pushed: target.pushed,
        checkout: target.slot.to_string_lossy().into_owned(),
        output_file: target.slot.join(OUTPUT_FILE).to_string_lossy().into_owned(),
        ..GateReport::default()
    }
}

fn running_report(
    seat: &str,
    def: &FleetDefinition,
    target: &Target,
    command: String,
    started_at: DateTime<Utc>,
    how: &str,
) -> GateReport {
    GateReport {
        command,
        how: how.to_string(),
        seconds: (Utc::now() - started_at).num_seconds(),
        started_at: Some(started_at),
        ..blank(seat, def, target)
    }
}

fn refused(seat: &str, def: &FleetDefinition, why: String) -> GateReport {
    GateReport {
        seat: seat.to_string(),
        command: def.gate.command.clone(),
        how: "refused".to_string(),
        refused: Some(why),
        ..GateReport::default()
    }
}

fn finished(
    seat: &str,
    def: &FleetDefinition,
    target: &Target,
    row: &TreeCheckRow,
    how: &str,
) -> GateReport {
    GateReport {
        command: row.command.clone(),
        how: how.to_string(),
        passed: row.passed() && row.state_after.as_deref() == Some(row.state.as_str()),
        exit_code: row.exit_code,
        seconds: row
            .finished_at
            .map(|done| (done - row.started_at).num_seconds())
            .unwrap_or_default(),
        output_tail: tail(&row.output),
        failure_inventory: inventory(&row.output),
        source_verified: row.state_after.as_deref() == Some(row.state.as_str()),
        output_file: output_reference(target, row),
        started_at: Some(row.started_at),
        ..blank(seat, def, target)
    }
}

fn output_reference(target: &Target, row: &TreeCheckRow) -> String {
    let log = completion_log(target, &row.command, &row.id.to_string());
    if log.is_file() {
        log.to_string_lossy().into_owned()
    } else {
        format!(
            "persisted gate result {} (execution log unavailable: {})",
            row.id,
            log.display()
        )
    }
}

fn tail(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    let start = lines.len().saturating_sub(TAIL_LINES);
    lines[start..].join("\n")
}

pub(super) fn matches_command(row: &TreeCheckRow, command: &str) -> bool {
    row.command == command
}

pub(super) fn running_commit(slot: &str) -> Option<String> {
    in_flight()
        .get(
            admission::key(std::path::Path::new(slot))
                .to_string_lossy()
                .as_ref(),
        )
        .map(|flight| flight.target.commit.clone())
}

fn now_running(slot: &str) -> Option<Outcome> {
    in_flight().get(slot).map(|flight| Outcome::Running {
        target: flight.target.clone(),
        command: flight.command.clone(),
        started_at: flight.started_at,
        how: completion::phase(&flight.ledger).unwrap_or(RUNNING),
    })
}

async fn current_status(
    persist: &PersistHandle,
    ledger: &str,
    slot: &str,
) -> Result<Outcome, String> {
    if let Some(running) = now_running(slot) {
        return Ok(running);
    }
    if let Some(row) = persist
        .latest_tree_check(ledger)
        .await
        .map_err(|err| err.to_string())?
        && row.finished_at.is_none()
    {
        return Err(format!(
            "gate receipt {} has no recorded completion or active runner; execution was interrupted. Inspect its execution-specific output logs before retrying; restart replay is not supported",
            row.id
        ));
    }
    Ok(Outcome::NotRun)
}

async fn obtain(
    runner: &Runner<'_>,
    def: &FleetDefinition,
    (seat, ask): (&str, &GateRequest),
    target: &Target,
) -> Result<Outcome, String> {
    let (persist, ledger) = (runner.persist, ledger_for(def, runner.run));
    completion::blocked(&ledger)?;
    let slot = admission::key(&target.slot).to_string_lossy().into_owned();
    if let Some(row) = persist
        .find_tree_check(&ledger, &target.commit)
        .await
        .map_err(|e| e.to_string())?
        .filter(|row| {
            matches_command(row, &def.gate.command)
                && (ask.status || super::operation::reusable_command(def, &def.gate.command))
        })
    {
        return Ok(Outcome::Finished(Box::new(row), REUSED));
    }
    if ask.status {
        return current_status(persist, &ledger, &slot).await;
    }
    if seat != ACCEPTANCE {
        return Err(
            "only the acceptance seat starts the full gate; ask again with the status form to read the verdict for this commit"
                .to_string(),
        );
    }
    let held = admission::hold(&target.slot).await;
    let running = in_flight().get(&slot).map(|f| {
        (
            f.ledger.clone(),
            f.target.commit.clone(),
            f.command.clone(),
            f.done.clone(),
        )
    });
    let (done, how) = match running {
        Some((other, _, _, _)) if other != ledger => {
            return Err(format!(
                "the isolated gate slot is busy with another fleet's gate ({other}); this request was not queued. Ask again after that gate completes"
            ));
        }
        Some((_, theirs, command, done))
            if theirs == target.commit && command == def.gate.command =>
        {
            (done, JOINED)
        }
        Some(_) => return Ok(now_running(&slot).unwrap_or(Outcome::NotRun)),
        None => (start(runner, def, seat, target).await?, FRESH),
    };
    drop(held);
    let waited = wait_for_result(done, ask, target).await;
    completion::blocked(&ledger)?;
    match waited? {
        Some(row) => Ok(Outcome::Finished(Box::new(row), how)),
        None => Ok(now_running(&slot).unwrap_or(Outcome::NotRun)),
    }
}

async fn wait_for_result(
    done: watch::Receiver<Option<TreeCheckRow>>,
    ask: &GateRequest,
    target: &Target,
) -> Result<Option<TreeCheckRow>, String> {
    let patience = Duration::from_secs(ask.wait_seconds.unwrap_or(DEFAULT_WAIT_SECONDS));
    let ended = match tokio::time::timeout(patience, wait_for(done.clone())).await {
        Ok(ended) => ended,
        Err(_) => {
            let _held = admission::hold(&target.slot).await;
            let completed = done.borrow().clone();
            if completed.is_none() {
                let slot = admission::key(&target.slot).to_string_lossy().into_owned();
                let mut sessions = waiting().entry(slot.clone()).or_default();
                if !sessions.contains(&ask.session) {
                    sessions.push(ask.session);
                }
                return Ok(None);
            }
            completed
        }
    };
    let row = ended.ok_or("the gate did not finish or its result could not be recorded")?;
    Ok(Some(row))
}

async fn wait_for(mut receiver: watch::Receiver<Option<TreeCheckRow>>) -> Option<TreeCheckRow> {
    loop {
        if let Some(row) = receiver.borrow().clone() {
            return Some(row);
        }
        receiver.changed().await.ok()?;
    }
}

async fn tell_waiting(
    persist: &PersistHandle,
    (def, seat): (&FleetDefinition, &str),
    target: &Target,
    row: &TreeCheckRow,
    sessions: Vec<Uuid>,
) {
    if sessions.is_empty() {
        return;
    }
    let body = format!(
        "[mike gate] {}",
        text(&finished(seat, def, target, row, "finished"))
    );
    let acceptance = super::plan::ControlSeats::recorded(def).map(|seats| seats.acceptance);
    for session in sessions
        .into_iter()
        .filter(|session| Some(*session) != acceptance)
    {
        if let Err(err) = persist
            .insert_message(Uuid::new_v4(), session, None, &body)
            .await
        {
            tracing::warn!(%session, %err, "fleet: could not mail the gate's verdict to the seat waiting for it");
        }
    }
}

#[cfg(test)]
#[path = "gate_provenance_tests.rs"]
mod provenance_tests;
