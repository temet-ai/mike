use chrono::{DateTime, Utc};
use serde::Serialize;

use super::changed::{ChangedPath, changed_paths};
use super::read_back::{reads_back, written_paths};
use super::tool_effect::{ObservationKind, READ_BACK_KIND, ToolEffect, classify};

#[derive(Debug, Clone, PartialEq)]
pub struct CallFact {
    pub use_id: i64,
    pub at: DateTime<Utc>,
    pub tool_name: String,
    pub input: serde_json::Value,
    pub has_result: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct Observation {
    pub observed_after_change: Option<bool>,
    pub last_change_use_id: Option<i64>,
    pub last_change_at: Option<DateTime<Utc>>,
    pub last_observation_use_id: Option<i64>,
    pub last_observation_at: Option<DateTime<Utc>>,
    pub observation_kind: Option<&'static str>,
    pub changed: Vec<ChangedPath>,
}

impl Observation {
    pub fn change_summary(&self) -> Option<String> {
        super::changed::summary(&self.changed)
    }
}

fn observing(call: &CallFact) -> Option<ObservationKind> {
    match classify(&call.tool_name, &call.input) {
        ToolEffect::Observing(kind) if call.has_result => Some(kind),
        _ => None,
    }
}

fn since_last_check(calls: &[CallFact], change_index: usize) -> &[CallFact] {
    let start = calls[..change_index]
        .iter()
        .rposition(|c| observing(c).is_some())
        .map_or(0, |i| i + 1);
    &calls[start..=change_index]
}

fn written_since_last_check(calls: &[CallFact], change_index: usize) -> Vec<String> {
    since_last_check(calls, change_index)
        .iter()
        .filter(|c| c.has_result)
        .flat_map(written_paths)
        .collect()
}

pub fn observe(calls: &[CallFact]) -> Observation {
    let last_change = calls.iter().enumerate().rev().find(|(_, c)| {
        c.has_result && matches!(classify(&c.tool_name, &c.input), ToolEffect::Mutating(_))
    });
    let after = last_change.map_or(0, |(i, _)| i + 1);
    let real = calls
        .iter()
        .skip(after)
        .filter_map(|c| observing(c).map(|kind| (c, kind.code())))
        .next_back();
    let read_back = || {
        let written = written_since_last_check(calls, last_change?.0);
        calls
            .iter()
            .skip(after)
            .rfind(|c| reads_back(c, &written))
            .map(|c| (c, READ_BACK_KIND))
    };
    let last_observation = real.or_else(read_back);
    Observation {
        observed_after_change: last_change.map(|_| last_observation.is_some()),
        last_change_use_id: last_change.map(|(_, c)| c.use_id),
        last_change_at: last_change.map(|(_, c)| c.at),
        last_observation_use_id: last_observation.map(|(c, _)| c.use_id),
        last_observation_at: last_observation.map(|(c, _)| c.at),
        observation_kind: last_observation.map(|(_, kind)| kind),
        changed: last_change
            .map(|(i, _)| changed_paths(since_last_check(calls, i)))
            .unwrap_or_default(),
    }
}
