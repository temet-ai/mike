use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::fetch::Window;
use super::findings::{Finding, findings, opportunity};
use super::promotion::{PromotionState, Publication};
use super::rows::{int, rows, text, truthy};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Totals {
    #[serde(default)]
    pub(super) sessions: i64,
    #[serde(default)]
    pub(super) requests: i64,
    #[serde(default)]
    pub(super) tool_calls: i64,
    #[serde(default)]
    pub(super) missing_usage: i64,
}

impl Totals {
    pub(super) fn minus(&self, earlier: &Totals) -> Totals {
        Totals {
            sessions: self.sessions - earlier.sessions,
            requests: self.requests - earlier.requests,
            tool_calls: self.tool_calls - earlier.tool_calls,
            missing_usage: self.missing_usage - earlier.missing_usage,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub(super) struct ProjectTotals {
    pub(super) sessions: i64,
    pub(super) requests: i64,
    pub(super) tool_calls: i64,
    pub(super) tool_errors: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct PreviousWindow {
    pub(super) start: String,
    pub(super) end: String,
    pub(super) totals: Totals,
    pub(super) delta: Totals,
}

#[derive(Debug, Deserialize)]
pub(super) struct PreviousReport {
    pub(super) start: String,
    pub(super) end: String,
    pub(super) totals: Totals,
}

impl PreviousReport {
    pub(super) fn against(self, current: &Totals) -> PreviousWindow {
        PreviousWindow {
            delta: current.minus(&self.totals),
            start: self.start,
            end: self.end,
            totals: self.totals,
        }
    }
}

#[derive(Debug, Default, Serialize)]
pub(super) struct Report {
    pub(super) version: u32,
    pub(super) start: String,
    pub(super) end: String,
    pub(super) data: Value,
    pub(super) projects: BTreeMap<String, ProjectTotals>,
    pub(super) totals: Totals,
    pub(super) findings: Vec<Finding>,
    pub(super) opportunities: Vec<Value>,
    pub(super) generated_at: String,
    pub(super) publication: Publication,
    pub(super) exposures: Value,
    pub(super) lesson_delivery: Option<Value>,
    pub(super) previous_window: Option<PreviousWindow>,
    pub(super) reuse_events: Vec<Value>,
}

fn project_totals(data: &Value) -> BTreeMap<String, ProjectTotals> {
    let mut projects: BTreeMap<String, ProjectTotals> = BTreeMap::new();
    for row in rows(data, "sessions") {
        if !truthy(row.get("is_global")) {
            projects.entry(text(row, "project")).or_default().sessions = int(row, "sessions");
        }
    }
    for row in rows(data, "requests") {
        projects.entry(text(row, "project")).or_default().requests += int(row, "requests");
    }
    for row in rows(data, "tools") {
        let entry = projects.entry(text(row, "project")).or_default();
        entry.tool_calls += int(row, "calls");
        entry.tool_errors += int(row, "errors");
    }
    projects
}

fn totals(data: &Value) -> Totals {
    let sum =
        |name: &str, key: &str| -> i64 { rows(data, name).iter().map(|row| int(row, key)).sum() };
    Totals {
        sessions: rows(data, "sessions")
            .iter()
            .find(|row| truthy(row.get("is_global")))
            .map_or(0, |row| int(row, "sessions")),
        requests: sum("requests", "requests"),
        tool_calls: sum("tools", "calls"),
        missing_usage: sum("requests", "missing_usage"),
    }
}

impl Report {
    pub(super) fn build(window: &Window, data: Value, promotion: &PromotionState) -> Self {
        Self {
            version: 1,
            start: window.start.to_rfc3339(),
            end: window.end.to_rfc3339(),
            projects: project_totals(&data),
            totals: totals(&data),
            findings: findings(&data, promotion),
            opportunities: rows(&data, "repeated_results")
                .iter()
                .map(opportunity)
                .collect(),
            data,
            ..Self::default()
        }
    }
}
