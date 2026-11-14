use crate::store::persistence::fleet_repo::FleetPersist as _;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, watch};
use tokio_util::task::TaskTracker;
use uuid::Uuid;

use super::check_blank::{blank, refused, withdrawn};
use super::check_report::{answer, report};
use super::check_run::{Started, baseline, requested, start_run, target};
pub use super::check_state::head;
pub use super::check_state::{
    found_in, parse_tree, short, short_commit, state_at, state_for, state_of,
};
use super::definition::FleetDefinition;
use super::diagnostics::Diagnostic;
use super::gate::Target;
use super::seated::Seated;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::TreeCheckRow;

pub const DEFAULT_WAIT_SECONDS: u64 = 0;
pub const MAX_WAIT_SECONDS: u64 = 90;
pub const WAITING: &str = "waiting";
pub const RUNNING: &str = "running";
pub const NOT_RUN: &str = "not run";
pub const WITHDRAWN: &str = "withdrawn";
const FRESH: &str = "fresh";
const JOINED: &str = "joined";
const REUSED: &str = "reused";
const ANOTHER_STATE: &str = "another state";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckRequest {
    pub session: Uuid,
    #[serde(default)]
    pub wait_seconds: Option<u64>,
    #[serde(default)]
    pub status: bool,
    #[serde(default)]
    pub test: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub commit: Option<String>,
    #[serde(default)]
    pub ruling: Option<Uuid>,
    #[serde(default)]
    pub withdraw: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Blocking {
    pub path: String,
    pub line: u32,
    pub text: String,
    pub seat: String,
    pub task: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckReport {
    #[serde(default)]
    pub receipt_id: Option<i64>,
    #[serde(default)]
    pub command_receipt: bool,
    #[serde(default)]
    pub baseline: bool,
    pub seat: String,
    pub command: String,
    pub state: String,
    #[serde(default)]
    pub commit: String,
    pub how: String,
    pub passed: bool,
    pub exit_code: Option<i32>,
    pub seconds: i64,
    pub askers: Vec<String>,
    pub mine: Vec<Diagnostic>,
    pub others: Vec<Blocking>,
    pub unowned: Vec<Diagnostic>,
    #[serde(default)]
    pub owned: Vec<String>,
    pub output_tail: String,
    pub refused: Option<String>,
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub triage: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub row: TreeCheckRow,
    pub commit: String,
    pub how: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asker {
    pub seat: String,
    pub owned: Vec<String>,
    pub everything: bool,
}

pub fn asker(def: &FleetDefinition, seated: &Seated, session: Uuid) -> Option<Asker> {
    if let Some(seat) = seated.seat(session) {
        let task = seat.task.as_deref().and_then(|id| def.tasks.get(id));
        return Some(match task {
            Some(task) => Asker {
                seat: seat.name.clone(),
                owned: task
                    .files
                    .iter()
                    .map(|f| super::checks::normal(f))
                    .collect(),
                everything: false,
            },
            None => Asker {
                seat: seat.name.clone(),
                owned: Vec::new(),
                everything: true,
            },
        });
    }
    let wanted = session.to_string();
    (def.reference.session.as_deref() == Some(wanted.as_str())).then(|| Asker {
        seat: "reference".to_string(),
        owned: Vec::new(),
        everything: true,
    })
}

struct InFlight {
    id: i64,
    running: bool,
    state: String,
    commit: String,
    started_at: DateTime<Utc>,
    done: watch::Receiver<Option<TreeCheckRow>>,
}

fn in_flight() -> &'static DashMap<String, InFlight> {
    static IN_FLIGHT: OnceLock<DashMap<String, InFlight>> = OnceLock::new();
    IN_FLIGHT.get_or_init(DashMap::new)
}

fn gates() -> &'static DashMap<String, Arc<Mutex<()>>> {
    static GATES: OnceLock<DashMap<String, Arc<Mutex<()>>>> = OnceLock::new();
    GATES.get_or_init(DashMap::new)
}

fn waiting() -> &'static DashMap<String, Vec<Uuid>> {
    static WAITING: OnceLock<DashMap<String, Vec<Uuid>>> = OnceLock::new();
    WAITING.get_or_init(DashMap::new)
}

pub fn mark_running(run: &str) {
    if let Some(mut flight) = in_flight().get_mut(run) {
        flight.running = true;
    }
}

pub fn is_running(run: &str) -> bool {
    in_flight().contains_key(run)
}

pub async fn landed(run: &str, receipt: i64) -> Vec<Uuid> {
    let gate = gates()
        .entry(run.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone();
    let _held = gate.lock().await;
    if in_flight()
        .remove_if(run, |_, flight| flight.id == receipt)
        .is_none()
        && in_flight().contains_key(run)
    {
        return Vec::new();
    }
    waiting().remove(run).map(|(_, s)| s).unwrap_or_default()
}

pub struct Runner<'a> {
    pub persist: &'a PersistHandle,
    pub tracker: &'a TaskTracker,
    pub run: &'a str,
    pub fleet: Uuid,
    pub decisions: Option<&'a super::decisions::FleetDecisions>,
}

enum Outcome {
    Finished(Box<Verdict>),
    Running {
        id: i64,
        how: &'static str,
        state: String,
        commit: String,
        started_at: DateTime<Utc>,
    },
    NotRun {
        state: String,
        commit: String,
    },
}

pub async fn check(
    runner: Runner<'_>,
    def: &FleetDefinition,
    seated: &Seated,
    request: &CheckRequest,
) -> CheckReport {
    let Some(who) = asker(def, seated, request.session) else {
        return refused(
            "?",
            &def.check,
            format!("session {} holds no seat in this fleet", request.session),
        );
    };
    if let Some(receipt) = request.withdraw {
        return withdrawn(&who.seat, &def.check, receipt);
    }
    let focused = match requested(def, runner.run, request) {
        Ok(focused) => focused,
        Err(why) => return refused(&who.seat, &def.check, why),
    };
    let (def, run) = match &focused {
        Some(f) => (&f.def, f.ledger.as_str()),
        None => (def, runner.run),
    };
    let (request, ledger) = match baseline::request(def, run, request).await {
        Ok(named) => named,
        Err(why) => return refused(&who.seat, &def.check, why),
    };
    let run = ledger.as_str();
    let runner = Runner { run, ..runner };
    let mut result = match obtain(&runner, def, &who, &request).await {
        Ok(Outcome::Finished(verdict)) if request.status => answer(def, &who, &verdict),
        Ok(Outcome::Finished(verdict)) => report(runner.persist, def, &who, *verdict).await,
        Ok(Outcome::Running {
            id,
            how,
            state,
            commit,
            started_at,
        }) => CheckReport {
            receipt_id: Some(id),
            state,
            commit,
            seconds: (Utc::now() - started_at).num_seconds(),
            started_at: Some(started_at),
            ..blank(&who.seat, &def.check, how)
        },
        Ok(Outcome::NotRun { state, commit }) => CheckReport {
            state,
            commit,
            ..blank(&who.seat, &def.check, NOT_RUN)
        },
        Err(why) => refused(&who.seat, &def.check, why),
    };
    result.baseline = baseline::is_baseline(run);
    result.command_receipt = super::check_run::is_command(run);
    result
}

fn now_running(run: &str) -> Option<Outcome> {
    in_flight().get(run).map(|flight| Outcome::Running {
        id: flight.id,
        how: super::check_run::completion::phase(run).unwrap_or(if flight.running {
            RUNNING
        } else {
            WAITING
        }),
        state: flight.state.clone(),
        commit: flight.commit.clone(),
        started_at: flight.started_at,
    })
}

async fn current_status(persist: &PersistHandle, run: &str) -> Result<Option<Outcome>, String> {
    super::check_run::completion::blocked(run)?;
    if let Some(running) = now_running(run) {
        return Ok(Some(running));
    }
    if let Some(row) = persist
        .fleet()
        .latest_tree_check(run)
        .await
        .map_err(|e| e.to_string())?
        && row.finished_at.is_none()
    {
        return Err(format!(
            "receipt {} has no completed result or active runner; execution was interrupted and is not resumed automatically. Submit the command again to retry",
            row.id
        ));
    }
    Ok(None)
}

fn wait_for_mail(run: &str, session: Uuid) {
    let mut sessions = waiting().entry(run.to_string()).or_default();
    if !sessions.contains(&session) {
        sessions.push(session);
    }
}

fn stop_waiting(run: &str, session: Uuid) {
    if let Some(mut sessions) = waiting().get_mut(run) {
        sessions.retain(|s| *s != session);
    }
}

async fn reused(
    persist: &PersistHandle,
    run: &str,
    state: &str,
    seat: Option<&str>,
) -> Result<Option<TreeCheckRow>, String> {
    let Some(row) = persist
        .fleet()
        .find_tree_check(run, state)
        .await
        .map_err(|e| e.to_string())?
    else {
        return Ok(None);
    };
    if let Some(seat) = seat {
        persist
            .fleet()
            .add_tree_check_asker(row.id, seat)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(Some(row))
}

fn finished(row: TreeCheckRow, at: &Target, how: &str) -> Outcome {
    Outcome::Finished(Box::new(Verdict {
        row,
        commit: at.commit.clone(),
        how: how.to_string(),
    }))
}

async fn obtain(
    runner: &Runner<'_>,
    def: &FleetDefinition,
    who: &Asker,
    ask: &CheckRequest,
) -> Result<Outcome, String> {
    let (persist, run) = (runner.persist, runner.run);
    let at = target(def, ask.commit.as_deref()).await?;
    let state = state_for(def, &at.commit).await?;
    if ask.status
        && let Some(running) = current_status(persist, run).await?
    {
        return Ok(running);
    }
    if (ask.status
        || (super::operation::reusable(def) && (ask.command.is_none() || ask.commit.is_some())))
        && let Some(row) = reused(
            persist,
            run,
            &state,
            (!ask.status).then_some(who.seat.as_str()),
        )
        .await?
    {
        return Ok(finished(row, &at, REUSED));
    }
    if ask.status {
        return Ok(Outcome::NotRun {
            state,
            commit: at.commit,
        });
    }
    let (done, how) =
        match join_or_start(runner, def, (&who.seat, &state, &at), Some(ask.session)).await {
            Ok(started) => started,
            Err(why) => {
                stop_waiting(run, ask.session);
                return Err(why);
            }
        };
    if how == ANOTHER_STATE {
        return Err("another tree state is already queued; this request was not queued. Its completion comes by mail; ask again for the new state afterward".into());
    }
    let patience = Duration::from_secs(
        ask.wait_seconds
            .unwrap_or(DEFAULT_WAIT_SECONDS)
            .min(MAX_WAIT_SECONDS),
    );
    if tokio::time::timeout(patience, wait_for(done.clone()))
        .await
        .is_err()
        && let Some(running) = now_running(run)
    {
        return Ok(running);
    }
    let row = wait_for(done)
        .await
        .ok_or("the check did not finish or its result could not be recorded")?;
    Ok(finished(row, &at, how))
}

pub async fn join_or_start(
    runner: &Runner<'_>,
    def: &FleetDefinition,
    (seat, state, at): (&str, &str, &Target),
    session: Option<Uuid>,
) -> Result<(watch::Receiver<Option<TreeCheckRow>>, &'static str), String> {
    let run = runner.run;
    let gate = gates()
        .entry(run.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone();
    let _held = gate.lock().await;
    super::check_run::completion::blocked(run)?;
    if let Some(session) = session {
        wait_for_mail(run, session);
    }
    let running = in_flight()
        .get(run)
        .map(|f| (f.id, f.state.clone(), f.done.clone()));
    match running {
        Some((id, theirs, done)) if theirs == state => {
            runner
                .persist
                .fleet()
                .add_tree_check_asker(id, seat)
                .await
                .map_err(|e| e.to_string())?;
            Ok((done, JOINED))
        }
        Some((id, _, done)) if !super::managed_queue::supersede(run, state).contains(&id) => {
            Ok((done, ANOTHER_STATE))
        }
        _ => {
            super::managed_queue::supersede(run, state);
            let managed = super::managed_command::begin_check(runner, def, state, seat).await?;
            let id = managed.id;
            let (sender, receiver) = watch::channel(None);
            in_flight().insert(
                run.to_string(),
                InFlight {
                    id,
                    running: false,
                    state: state.to_string(),
                    commit: at.commit.clone(),
                    started_at: Utc::now(),
                    done: receiver.clone(),
                },
            );
            let started = Started {
                id,
                state: state.to_string(),
                at: at.clone(),
                managed,
            };
            start_run(runner, def.clone(), started, sender);
            Ok((receiver, FRESH))
        }
    }
}

async fn wait_for(mut receiver: watch::Receiver<Option<TreeCheckRow>>) -> Option<TreeCheckRow> {
    loop {
        if let Some(row) = receiver.borrow().clone() {
            return Some(row);
        }
        receiver.changed().await.ok()?;
    }
}

#[cfg(test)]
#[path = "check_flight_tests.rs"]
pub mod flight_tests;
