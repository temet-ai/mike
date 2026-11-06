use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use arc_swap::ArcSwap;
use chrono::{DateTime, Utc};
use serde::Serialize;

use super::runner::RunResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Trigger {
    Schedule,
    Manual,
}

impl Trigger {
    pub(super) fn as_db(self) -> &'static str {
        match self {
            Self::Schedule => "schedule",
            Self::Manual => "manual",
        }
    }

    pub(super) fn from_db(value: &str) -> Self {
        if value == "manual" {
            Self::Manual
        } else {
            Self::Schedule
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunRecord {
    pub(crate) trigger: Trigger,
    pub(crate) started_at: DateTime<Utc>,
    pub(crate) finished_at: DateTime<Utc>,
    pub(crate) duration_ms: u64,
    pub(crate) result: RunResult,
    pub(crate) tail: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct JobStatus {
    pub(crate) started: bool,
    pub(crate) enabled: bool,
    pub(crate) held_elsewhere: bool,
    pub(crate) root: Option<PathBuf>,
    pub(crate) running_since: Option<DateTime<Utc>>,
    pub(crate) run_requested_at: Option<DateTime<Utc>>,
    pub(crate) last_started_at: Option<DateTime<Utc>>,
    pub(crate) last_scheduled_started_at: Option<DateTime<Utc>>,
    pub(crate) last_run: Option<RunRecord>,
    pub(crate) consecutive_failures: u32,
    pub(crate) retry_after: Option<DateTime<Utc>>,
    pub(crate) next_run_at: Option<DateTime<Utc>>,
}

type StatusMap = BTreeMap<&'static str, JobStatus>;

static STATUS: OnceLock<ArcSwap<StatusMap>> = OnceLock::new();

fn cell() -> &'static ArcSwap<StatusMap> {
    STATUS.get_or_init(|| ArcSwap::from_pointee(StatusMap::new()))
}

pub(crate) fn snapshot(name: &str) -> JobStatus {
    cell().load().get(name).cloned().unwrap_or_default()
}

#[cfg(test)]
pub(crate) fn replace_for_tests(name: &'static str, status: &JobStatus) {
    update(name, |s| *s = status.clone());
}

pub(super) fn update(name: &'static str, change: impl Fn(&mut JobStatus)) {
    cell().rcu(|old| {
        let mut next = (**old).clone();
        change(next.entry(name).or_default());
        Arc::new(next)
    });
}
