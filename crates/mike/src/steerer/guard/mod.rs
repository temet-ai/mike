pub(crate) mod bars;
pub(crate) mod calibration_input;
pub(crate) mod call;
pub(crate) mod context;
pub(crate) mod exempt;
mod flow;
pub(crate) mod gates;
pub(crate) mod holdback;
pub(crate) mod input;
pub(crate) mod label;
mod label_call;
pub(crate) mod label_rules;
pub(crate) mod live;
pub(crate) mod live_decision;
pub(crate) mod metrics;
pub(crate) mod nudges;
pub(crate) mod policy;
pub(crate) mod promotion;
pub(crate) mod question_gates;
pub(crate) mod reach;
pub(crate) mod record;
pub(crate) mod replay;
pub(crate) mod schema;
mod semantic_evidence;
pub(crate) mod step_class;
#[cfg(any(test, feature = "test-seams"))]
pub mod test_seams;
pub(crate) mod threshold_select;

#[cfg(test)]
#[path = "tests/gates_tests.rs"]
mod gates_tests;
#[cfg(test)]
#[path = "tests/label_rules_tests.rs"]
mod label_rules_tests;
#[cfg(test)]
#[path = "tests/metrics_tests.rs"]
mod metrics_tests;
#[cfg(test)]
#[path = "tests/step_class_tests.rs"]
mod step_class_tests;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::llm::OpenRouterClient;
use crate::steerer::note_policy::NoteLimits;
use crate::steerer::verdict::Need;
use crate::store::persistence::PersistHandle;

pub(crate) use flow::{evaluate, evaluate_idle, finish};

pub const GUARD_RUBRIC_VERSION: &str = "guard-v1-2026-09";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Question {
    Relapse,
    UnbackedWait,
    AuthorisedHandback,
    UnobservedDone,
}

impl Question {
    pub const ALL: [Self; 4] = [
        Self::Relapse,
        Self::UnbackedWait,
        Self::AuthorisedHandback,
        Self::UnobservedDone,
    ];

    pub const PRIORITY: [Self; 4] = [
        Self::Relapse,
        Self::UnbackedWait,
        Self::UnobservedDone,
        Self::AuthorisedHandback,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::Relapse => "relapse",
            Self::UnbackedWait => "unbacked_wait",
            Self::AuthorisedHandback => "authorised_handback",
            Self::UnobservedDone => "unobserved_done",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|q| q.code() == raw)
    }

    pub fn has_quorum(self) -> bool {
        matches!(self, Self::Relapse | Self::UnbackedWait)
    }

    pub fn need(self) -> Need {
        match self {
            Self::Relapse | Self::UnbackedWait | Self::UnobservedDone => Need::High,
            Self::AuthorisedHandback => Need::Medium,
        }
    }

    pub fn rank(self) -> usize {
        Self::PRIORITY
            .iter()
            .position(|q| *q == self)
            .unwrap_or(Self::PRIORITY.len())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    TurnEnd,
    Quiescence,
}

impl Trigger {
    pub fn code(self) -> &'static str {
        match self {
            Self::TurnEnd => "turn_end",
            Self::Quiescence => "quiescence",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GuardStep {
    Continue,
    StopWindow,
}

pub(crate) struct GuardTurn<'a> {
    pub(crate) persist: &'a PersistHandle,
    pub(crate) llm: &'a OpenRouterClient,
    pub(crate) steerer_id: i64,
    pub(crate) bundle_id: Option<i64>,
    pub(crate) session_db_id: i64,
    pub(crate) session_uuid: Option<uuid::Uuid>,
    pub(crate) operator_turn_id: i64,
    pub(crate) trigger: Trigger,
    pub(crate) trigger_request_id: Option<i64>,
    pub(crate) agent_id: Option<String>,
    pub(crate) persona: Option<String>,
    pub(crate) text: String,
    pub(crate) verifier_pattern: Option<String>,
    pub(crate) toggle_on: bool,
    pub(crate) turn_judge_live: bool,
    pub(crate) note_limits: Option<NoteLimits>,
    pub(crate) now: DateTime<Utc>,
}
