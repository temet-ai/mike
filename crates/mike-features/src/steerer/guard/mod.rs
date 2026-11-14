pub mod bars;
pub mod calibration_input;
pub mod call;
pub mod context;
pub mod exempt;
pub mod flow;
pub mod gates;
pub mod holdback;
pub mod input;
pub mod jev_seam;
pub mod label;
pub mod label_call;
pub mod label_rules;
pub mod live;
pub mod live_decision;
pub mod metrics;
pub mod nudges;
pub mod policy;
pub mod precheck_seam;
pub mod promotion;
pub mod question_gates;
pub mod reach;
pub mod record;
pub mod replay;
pub mod schema;
pub mod semantic_evidence;
pub mod step_class;
#[cfg(any(test, feature = "test-seams"))]
pub mod test_seams;
pub mod threshold_select;

#[cfg(test)]
#[path = "tests/gates_tests.rs"]
pub mod gates_tests;
#[cfg(test)]
#[path = "tests/label_rules_tests.rs"]
pub mod label_rules_tests;
#[cfg(test)]
#[path = "tests/metrics_tests.rs"]
pub mod metrics_tests;
#[cfg(test)]
#[path = "tests/step_class_tests.rs"]
pub mod step_class_tests;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::llm::OpenRouterClient;
use crate::steerer::note_policy::NoteLimits;
use crate::steerer::verdict::Need;
use crate::store::persistence::PersistHandle;

pub use flow::{evaluate, evaluate_idle, finish};

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
pub enum GuardStep {
    Continue,
    StopWindow,
}

pub struct GuardTurn<'a> {
    pub persist: &'a PersistHandle,
    pub llm: &'a OpenRouterClient,
    pub decisions: Option<precheck_seam::DecisionsTurn>,
    pub jev: jev_seam::GuardDeciders,
    pub steerer_id: i64,
    pub bundle_id: Option<i64>,
    pub session_db_id: i64,
    pub session_uuid: Option<uuid::Uuid>,
    pub operator_turn_id: i64,
    pub trigger: Trigger,
    pub trigger_request_id: Option<i64>,
    pub agent_id: Option<String>,
    pub persona: Option<String>,
    pub text: String,
    pub verifier_pattern: Option<String>,
    pub toggle_on: bool,
    pub turn_judge_live: bool,
    pub note_limits: Option<NoteLimits>,
    pub now: DateTime<Utc>,
}
