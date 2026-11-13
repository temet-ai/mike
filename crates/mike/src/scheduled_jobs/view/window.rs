use serde::Serialize;

use super::super::jobs::Job;
use super::super::schedule::Period;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum JobGroup {
    LearningAndMemory,
    Measurement,
    BuildsAndTests,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct JobWindow {
    pub(crate) period: &'static str,
    pub(crate) weekday: Option<String>,
    pub(crate) hours_utc: Vec<u32>,
    pub(crate) minute: u32,
}

pub(crate) fn group_of(job: &Job) -> JobGroup {
    match job.name {
        "compressor_loop" | "usage_audit" => JobGroup::Measurement,
        "cargo_sweep" | "nightly_tests" => JobGroup::BuildsAndTests,
        _ => JobGroup::LearningAndMemory,
    }
}

pub(crate) fn window_of(job: &Job) -> JobWindow {
    let cadence = &job.cadence;
    JobWindow {
        period: match cadence.period {
            Period::Hour => "hour",
            Period::Day => "day",
            Period::Week => "week",
        },
        weekday: cadence.weekday.map(|day| day.to_string()),
        hours_utc: cadence.hours.to_vec(),
        minute: cadence.minute,
    }
}
