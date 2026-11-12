mod candidates;
pub(crate) mod first_ten;
pub(crate) mod grade;
mod lesson_pairs;
mod load;
#[cfg(test)]
mod load_tests;
pub(crate) mod run_now;
pub(crate) mod tune;

pub(crate) use run_now::{RunNow, run_pass_now};

use std::time::Duration;

use chrono::{DateTime, TimeZone, Timelike, Utc};
use tokio::time::{MissedTickBehavior, interval};

use crate::analysis::rerank::RerankNightly as QuietBudget;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::search_grades::UngradedEvent;
use candidates::candidates_phase;
use grade::{Graded, grade_event};
use lesson_pairs::pairs_phase;
use tune::{Tuned, tune_if_due};

const LEASE_ROLE: &str = "rerank_nightly";
const TICK_INTERVAL: Duration = Duration::from_secs(600);
const GRADE_BATCH: i64 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tick {
    TuningOff,
    OutsideWindow,
    Busy,
    NotLeader,
    Ran(Pass),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Pass {
    pub(crate) graded: usize,
    pub(crate) thread_candidates: usize,
    pub(crate) lesson_pairs: usize,
    pub(crate) stopped: Option<Stop>,
    pub(crate) tuned: Option<Tuned>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stop {
    MachineBusy,
    RerankerFailed,
    DatabaseFailed,
    MoreToGrade,
}

pub(crate) fn spawn_rerank_nightly(state: &AppState) {
    let Some(persist) = state.persist().cloned() else {
        tracing::debug!("rerank_nightly: no persistence — not spawning");
        return;
    };
    let state = state.clone();
    let shutdown = state.shutdown();
    let tracker = state.tracker();
    tracker.spawn(async move {
        let mut ticker = interval(TICK_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                () = shutdown.cancelled() => break,
                _ = ticker.tick() => {
                    run_tick(&state, &persist, Utc::now()).await;
                }
            }
        }
    });
    tracing::info!(
        interval_secs = TICK_INTERVAL.as_secs(),
        "rerank_nightly: worker spawned"
    );
}

pub(crate) async fn run_tick(
    state: &AppState,
    persist: &PersistHandle,
    now: DateTime<Utc>,
) -> Tick {
    if !state.rerank_service().current_config().tuning_enabled {
        return Tick::TuningOff;
    }
    let budget = quiet_budget(state);
    if !budget.in_window(now.hour()) {
        return Tick::OutsideWindow;
    }
    let idle = LoadIdle { state };
    if !idle.is_idle().await {
        return Tick::Busy;
    }
    if !persist.holds_worker_role(LEASE_ROLE, TICK_INTERVAL).await {
        return Tick::NotLeader;
    }
    let pass = night_pass(state, persist, &budget, now, &idle).await;
    report(&pass);
    Tick::Ran(pass)
}

pub(crate) async fn night_pass(
    state: &AppState,
    persist: &PersistHandle,
    budget: &QuietBudget,
    now: DateTime<Utc>,
    idle: &impl Idle,
) -> Pass {
    let mut pass = Pass::default();
    pass.stopped = grade_phase(state, persist, idle, &mut pass.graded).await;
    if pass.stopped.is_none() {
        let window_start = window_start(now, budget.start_hour_utc);
        pass.stopped = tune_phase(state, persist, window_start, now, &mut pass.tuned).await;
    }
    if pass.stopped.is_none() {
        pass.stopped = candidates_phase(state, persist, idle, &mut pass.thread_candidates).await;
    }
    if pass.stopped.is_none() {
        pass.stopped = pairs_phase(state, persist, idle, &mut pass.lesson_pairs).await;
    }
    pass
}

async fn tune_phase(
    state: &AppState,
    persist: &PersistHandle,
    window_start: DateTime<Utc>,
    now: DateTime<Utc>,
    tuned: &mut Option<Tuned>,
) -> Option<Stop> {
    match tune_if_due(state, persist, window_start, now).await {
        Ok(done) => {
            *tuned = Some(done);
            None
        }
        Err(error) => Some(database_failed(&error, "tuning")),
    }
}

fn database_failed(error: &dyn std::fmt::Display, step: &'static str) -> Stop {
    tracing::warn!(%error, step, "rerank_nightly: a database step failed");
    Stop::DatabaseFailed
}

async fn grade_phase(
    state: &AppState,
    persist: &PersistHandle,
    idle: &impl Idle,
    graded: &mut usize,
) -> Option<Stop> {
    let events = match persist.ungraded_search_events(GRADE_BATCH).await {
        Ok(events) => events,
        Err(error) => {
            tracing::warn!(%error, "rerank_nightly: could not list ungraded searches");
            return Some(Stop::DatabaseFailed);
        }
    };
    let batch_was_full = i64::try_from(events.len()).is_ok_and(|n| n >= GRADE_BATCH);
    match grade_events(state, persist, events, idle, graded).await {
        None if batch_was_full => Some(Stop::MoreToGrade),
        stopped => stopped,
    }
}

async fn grade_events(
    state: &AppState,
    persist: &PersistHandle,
    events: Vec<UngradedEvent>,
    idle: &impl Idle,
    graded: &mut usize,
) -> Option<Stop> {
    for event in events {
        if !idle.is_idle().await {
            return Some(Stop::MachineBusy);
        }
        match grade_event(state, persist, &event).await {
            Ok(Graded::Recorded) => *graded += 1,
            Ok(Graded::RerankerFailed) => return Some(Stop::RerankerFailed),
            Err(error) => {
                tracing::warn!(%error, event = event.event_id, "rerank_nightly: grading failed");
                return Some(Stop::DatabaseFailed);
            }
        }
    }
    None
}

pub(crate) trait Idle {
    fn is_idle(&self) -> impl Future<Output = bool> + Send;
}

struct LoadIdle<'a> {
    state: &'a AppState,
}

impl Idle for LoadIdle<'_> {
    async fn is_idle(&self) -> bool {
        let ceiling = quiet_budget(self.state).max_load_per_cpu;
        load::under_ceiling(load::load_per_cpu(), ceiling)
    }
}

pub(crate) fn quiet_budget(state: &AppState) -> QuietBudget {
    state.rerank_service().current_config().nightly
}

pub(crate) fn window_start(now: DateTime<Utc>, start_hour: u32) -> DateTime<Utc> {
    let today = now.date_naive().and_hms_opt(start_hour, 0, 0);
    let start = today.map(|t| Utc.from_utc_datetime(&t)).unwrap_or(now);
    if start > now {
        start - chrono::Duration::days(1)
    } else {
        start
    }
}

fn report(pass: &Pass) {
    if pass.graded == 0
        && pass.tuned.is_none()
        && pass.thread_candidates == 0
        && pass.lesson_pairs == 0
    {
        return;
    }
    tracing::info!(
        graded = pass.graded,
        thread_candidates = pass.thread_candidates,
        lesson_pairs = pass.lesson_pairs,
        stopped = ?pass.stopped,
        tuned = ?pass.tuned,
        "rerank_nightly: pass finished"
    );
}
