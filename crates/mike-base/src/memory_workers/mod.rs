use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use arc_swap::ArcSwap;
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use serde_json::Value;

#[cfg(test)]
mod tests;

pub const SUPPLY_VERDICT: &str = "supply_verdict";
pub const RERANK_NIGHTLY: &str = "rerank_nightly";
pub const LESSON_PROPOSER: &str = "lesson_proposer";

pub const CATCH_UP_AFTER_HOURS: i64 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Skip {
    Off,
    OutsideWindow,
    Busy,
    NotLeader,
    NoClient,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Outcome {
    Ran { counts: Value },
    Skipped { reason: Skip },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct WorkerStatus {
    pub last_ran_at: Option<DateTime<Utc>>,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub running_since: Option<DateTime<Utc>>,
    pub last_outcome: Option<Outcome>,
    pub seeded: bool,
}

type StatusMap = BTreeMap<&'static str, WorkerStatus>;

static STATUS: OnceLock<ArcSwap<StatusMap>> = OnceLock::new();

fn cell() -> &'static ArcSwap<StatusMap> {
    STATUS.get_or_init(|| ArcSwap::from_pointee(StatusMap::new()))
}

fn update(worker: &'static str, change: impl Fn(&mut WorkerStatus)) {
    cell().rcu(|old| {
        let mut next = (**old).clone();
        change(next.entry(worker).or_default());
        Arc::new(next)
    });
}

pub fn snapshot(worker: &str) -> WorkerStatus {
    cell().load().get(worker).cloned().unwrap_or_default()
}

pub fn seed(worker: &'static str, newest_row: Option<DateTime<Utc>>) {
    update(worker, |status| {
        status.seeded = true;
        status.last_ran_at = status.last_ran_at.max(newest_row);
    });
}

pub fn mark_running(worker: &'static str, since: DateTime<Utc>) {
    update(worker, |status| status.running_since = Some(since));
}

pub fn record_ran(worker: &'static str, at: DateTime<Utc>, counts: &Value) {
    update(worker, |status| {
        status.running_since = None;
        status.last_ran_at = Some(at);
        status.last_checked_at = Some(at);
        status.last_outcome = Some(Outcome::Ran {
            counts: counts.clone(),
        });
    });
    tracing::info!(worker, %counts, "memory worker: pass ran");
}

pub fn record_skip(worker: &'static str, at: DateTime<Utc>, reason: Skip) {
    update(worker, |status| {
        status.running_since = None;
        status.last_checked_at = Some(at);
        status.last_outcome = Some(Outcome::Skipped { reason });
    });
    tracing::debug!(worker, ?reason, "memory worker: tick skipped");
}

pub fn catch_up_due(last_ran_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    last_ran_at.is_none_or(|at| now - at >= Duration::hours(CATCH_UP_AFTER_HOURS))
}

pub fn gate(
    in_window: bool,
    last_ran_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    idle: bool,
) -> Option<Skip> {
    if !in_window && !catch_up_due(last_ran_at, now) {
        return Some(Skip::OutsideWindow);
    }
    (!idle).then_some(Skip::Busy)
}
