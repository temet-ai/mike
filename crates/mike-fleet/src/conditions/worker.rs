use std::sync::LazyLock;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use super::decide::{Decision, decide};
use super::fire::{Outcome, run_action};
use super::words;
use crate::base_state::BaseState;
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::conditions::{ConditionRow, Fired, Spec};
use crate::store::persistence::fleet_repo::FleetPersist as _;

const PASS_GAP: Duration = Duration::from_secs(10);
const IDLE_WAIT: Duration = Duration::from_secs(60);
const NONE_ARMED_WAIT: Duration = Duration::from_secs(600);
const WATCHED: [RowChangedTable; 4] = [
    RowChangedTable::Requests,
    RowChangedTable::Sessions,
    RowChangedTable::MailboxMessages,
    RowChangedTable::LaunchFleets,
];

static ARMED: LazyLock<Notify> = LazyLock::new(Notify::new);

pub fn armed() {
    ARMED.notify_one();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wait {
    Watch,
    Until(Option<DateTime<Utc>>),
}

pub fn spawn_conditions_worker(state: &BaseState) {
    let Some(persist) = state.persist().cloned() else {
        tracing::debug!("conditions: no persistence, not spawning");
        return;
    };
    state
        .tracker
        .spawn(run(state.clone(), persist, state.shutdown()));
    tracing::info!("conditions: worker spawned");
}

async fn run(state: BaseState, persist: PersistHandle, shutdown: CancellationToken) {
    let feed = state.change_feed.clone();
    loop {
        let since = feed.generation();
        let wait = if state.feature_toggle_overlay.load().conditions {
            pass(&state, &persist).await
        } else {
            Wait::Watch
        };
        let carry_on = match wait {
            Wait::Watch => watch(&feed, since, &shutdown).await,
            Wait::Until(at) => sleep_until_armed(at, &shutdown).await,
        };
        if !carry_on {
            break;
        }
    }
}

async fn watch(
    feed: &crate::change_feed::ChangeFeed,
    since: crate::change_feed::Generation,
    shutdown: &CancellationToken,
) -> bool {
    tokio::select! {
        biased;
        () = shutdown.cancelled() => return false,
        () = tokio::time::sleep(PASS_GAP) => {}
    }
    tokio::select! {
        biased;
        () = shutdown.cancelled() => false,
        _ = feed.wait_for_change(&WATCHED, since, IDLE_WAIT) => true,
    }
}

async fn sleep_until_armed(at: Option<DateTime<Utc>>, shutdown: &CancellationToken) -> bool {
    tokio::select! {
        biased;
        () = shutdown.cancelled() => false,
        () = ARMED.notified() => true,
        () = tokio::time::sleep(until(at, Utc::now())) => true,
    }
}

pub fn until(at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Duration {
    at.map_or(NONE_ARMED_WAIT, |at| {
        (at - now)
            .to_std()
            .unwrap_or(Duration::ZERO)
            .min(NONE_ARMED_WAIT)
    })
}

async fn pass(state: &BaseState, persist: &PersistHandle) -> Wait {
    let Some(machine) = state.resolve_host_machine_uuid().await else {
        return Wait::Watch;
    };
    match persist.fleet().next_armed_condition_at(machine).await {
        Ok(None) => return Wait::Until(None),
        Ok(Some(at)) if at > Utc::now() => return Wait::Until(Some(at)),
        Ok(Some(_)) => {}
        Err(error) => warn_failed("reading armed conditions", &error.to_string(), None),
    }
    let due = persist.fleet().due_conditions(machine).await;
    let rows = due.unwrap_or_else(|error| {
        warn_failed("reading due conditions", &error.to_string(), None);
        Vec::new()
    });
    for row in rows {
        settle(state, persist, &row).await;
    }
    Wait::Watch
}

async fn settle(state: &BaseState, persist: &PersistHandle, row: &ConditionRow) {
    let now = Utc::now();
    let held = match Spec::parse(&row.kind, &row.spec) {
        Ok(Spec::Words(spec)) => words::judge(state, persist, row, &spec, now).await,
        _ => persist
            .fleet()
            .condition_holds(row, now)
            .await
            .map_err(|e| e.to_string()),
    };
    match held {
        Ok(held) => {
            if let Decision::Fire(fired) = decide(held, row.deadline, now) {
                fire(state, persist, row, fired).await;
            }
        }
        Err(error) => record_failure(persist, row, &error).await,
    }
}

async fn fire(state: &BaseState, persist: &PersistHandle, row: &ConditionRow, fired: Fired) {
    if !claim(persist, row, fired).await {
        return;
    }
    match run_action(state, persist, row, fired).await {
        Ok(outcome) => record_outcome(persist, row, fired, &outcome).await,
        Err(error) => record_failure(persist, row, &error).await,
    }
}

async fn claim(persist: &PersistHandle, row: &ConditionRow, fired: Fired) -> bool {
    let claim = serde_json::json!({ "via": "claimed", "missed": fired.state() == "missed" });
    let claimed = persist
        .fleet()
        .mark_condition_fired(row.id, fired, &claim)
        .await;
    claimed
        .unwrap_or_else(|error| {
            warn_failed("claiming the firing", &error.to_string(), Some(row));
            None
        })
        .is_some()
}

async fn record_outcome(
    persist: &PersistHandle,
    row: &ConditionRow,
    fired: Fired,
    outcome: &Outcome,
) {
    let stored = persist
        .fleet()
        .set_condition_outcome(row.id, &outcome.to_json())
        .await;
    if let Err(error) = stored {
        warn_failed("recording the outcome", &error.to_string(), Some(row));
    }
    log_fired(row, fired.state(), outcome.via);
}

async fn record_failure(persist: &PersistHandle, row: &ConditionRow, error: &str) {
    let marked = persist
        .fleet()
        .mark_condition_failed_attempt(row.id, error)
        .await;
    match marked {
        Ok(Some(updated)) if updated.attempts == 1 || updated.state == "failed" => {
            log_attempt_failed(row, updated.attempts, &updated.state, error);
        }
        Ok(_) => {}
        Err(db_error) => warn_failed(
            "recording a failed attempt",
            &db_error.to_string(),
            Some(row),
        ),
    }
}

fn log_fired(row: &ConditionRow, state: &str, via: &str) {
    tracing::info!(condition = %row.condition_uuid, state, via, "conditions: action ran");
}

fn log_attempt_failed(row: &ConditionRow, attempts: i32, state: &str, error: &str) {
    tracing::warn!(condition = %row.condition_uuid, attempts, state, error, "conditions: attempt failed");
}

fn warn_failed(step: &str, error: &str, row: Option<&ConditionRow>) {
    let condition = row
        .map(|r| r.condition_uuid.to_string())
        .unwrap_or_default();
    tracing::warn!(step, error, condition, "conditions: a step failed");
}
