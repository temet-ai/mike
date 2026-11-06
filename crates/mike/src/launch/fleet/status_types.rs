use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::checks::Missing;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FleetStatus {
    pub fleet: String,
    pub slug: String,
    pub state: String,
    pub run_id: Option<String>,
    pub goal: String,
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub finished_at: Option<DateTime<Utc>>,
    pub now: DateTime<Utc>,
    pub missing: Vec<Missing>,
    #[serde(default)]
    pub accepted: bool,
    #[serde(default)]
    pub obligations: Vec<AcceptanceObligation>,
    pub criteria: Vec<CriterionStatus>,
    pub tasks: Vec<TaskStatus>,
    pub seats: Vec<SeatStatus>,
    pub verdicts: Vec<Verdict>,
    pub blocked: Vec<Blocked>,
    #[serde(default)]
    pub needs_operator: Vec<NeedsOperator>,
    #[serde(default)]
    pub every_seat_idle_seconds: Option<i64>,
    #[serde(default)]
    pub tree_broken: Option<super::shared_tree::TreeBroken>,
    #[serde(default)]
    pub problems: Vec<ProblemStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProblemStatus {
    pub trigger: String,
    pub task: Option<String>,
    pub opened_at: DateTime<Utc>,
    pub detail: String,
    pub decisions: Vec<String>,
    pub escalated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeedsOperator {
    pub at: DateTime<Utc>,
    pub seat: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CriterionStatus {
    pub id: String,
    pub text: String,
    pub tasks: usize,
    pub passed: usize,
    #[serde(default)]
    pub ready: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskStatus {
    pub id: String,
    pub name: String,
    pub discipline: String,
    pub criteria: Vec<String>,
    pub seat: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seat: Option<String>,
    pub state: String,
    pub passes: usize,
    pub fails: usize,
    #[serde(default)]
    pub rounds: usize,
    pub last: Option<Verdict>,
    #[serde(default)]
    pub passed_criteria: Vec<String>,
    #[serde(default)]
    pub ready: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeatStatus {
    pub seat: String,
    pub role: String,
    pub model: String,
    pub state: String,
    #[serde(default)]
    pub session: Option<uuid::Uuid>,
    #[serde(default)]
    pub goal: Option<String>,
    pub context_tokens: Option<i64>,
    pub requests: i64,
    pub last_request_at: Option<DateTime<Utc>>,
    pub opened_at: Option<DateTime<Utc>>,
    pub closed: bool,
    #[serde(default)]
    pub first_request_cache: Option<FirstRequestCache>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Verdict {
    pub at: DateTime<Utc>,
    pub verdict: String,
    pub title: String,
    pub tasks: Vec<String>,
    #[serde(default)]
    pub criteria: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Blocked {
    pub seat: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SeatFacts {
    pub seat: Option<String>,
    pub session: Option<uuid::Uuid>,
    pub role: Option<String>,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub opened_at: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
    pub requests: i64,
    pub last_request_at: Option<DateTime<Utc>>,
    pub context_tokens: Option<i64>,
    pub live_state: Option<String>,
    pub last_status: Option<i16>,
    pub first_request_cache: Option<FirstRequestCache>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NoteFacts {
    pub note_uuid: uuid::Uuid,
    pub session_uuid: uuid::Uuid,
    pub role: Option<String>,
    pub body: String,
    pub tags: Vec<String>,
    pub updated_at: DateTime<Utc>,
    pub kind: String,
    pub status: Option<String>,
    pub title: String,
    pub data: Value,
    pub created_at: DateTime<Utc>,
    pub seat: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FleetHeader {
    pub fleet: String,
    pub slug: String,
    pub state: String,
    pub run_id: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub now: DateTime<Utc>,
    pub missing: Vec<Missing>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AcceptanceObligation {
    pub note_id: uuid::Uuid,
    pub session_uuid: uuid::Uuid,
    pub obligation_id: String,
    #[serde(default)]
    pub ruling_proof: Vec<super::ruling_proof::RulingProof>,
    pub title: String,
    pub body: String,
    pub data: Value,
    pub tasks: Vec<String>,
    pub criteria: Vec<String>,
    pub outcome: String,
    pub required: bool,
    pub issue: Option<String>,
    pub at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AcceptanceObligation {
    pub(super) fn accepted(&self) -> bool {
        self.outcome == "accepted"
            && self.issue.is_none()
            && (self.data.get("ruling").is_none()
                || (!self.tasks.is_empty()
                    && self.tasks.iter().all(|task| self.accepted_for(task))))
    }

    pub(super) fn accepted_for(&self, task: &str) -> bool {
        self.outcome == "accepted"
            && self.issue.is_none()
            && (self.data.get("ruling").is_none()
                || self
                    .ruling_proof
                    .iter()
                    .any(|proof| proof.task == task && proof.passed()))
    }

    pub(super) fn covers(&self, task: &str, criterion: &str) -> bool {
        (!self.tasks.is_empty() || !self.criteria.is_empty())
            && (self.tasks.is_empty() || self.tasks.iter().any(|id| id == task))
            && (self.criteria.is_empty() || self.criteria.iter().any(|id| id == criterion))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirstRequestCache {
    pub read: i64,
    pub written: i64,
    pub cold: bool,
}

pub fn first_request_cache(
    forked: bool,
    read: Option<i64>,
    written: Option<i64>,
) -> Option<FirstRequestCache> {
    if read.is_none() && written.is_none() {
        return None;
    }
    let read = read.unwrap_or(0);
    let written = written.unwrap_or(0);
    Some(FirstRequestCache {
        read,
        written,
        cold: forked && written > read,
    })
}

pub fn label(cache: Option<&FirstRequestCache>) -> String {
    match cache {
        None => String::new(),
        Some(c) => {
            let marker = if c.cold { " COLD START" } else { "" };
            format!(
                "first request read {}k wrote {}k{marker}",
                (c.read + 500) / 1000,
                (c.written + 500) / 1000
            )
        }
    }
}

#[cfg(test)]
#[path = "status_types_tests.rs"]
mod status_types_tests;
