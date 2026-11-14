use serde::{Deserialize, Serialize};

use crate::llm::decisions::record::DecisionMode;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Off,
    Shadow,
    #[default]
    On,
}

impl Mode {
    pub fn decision_mode(self) -> Option<DecisionMode> {
        match self {
            Self::Off => None,
            Self::Shadow => Some(DecisionMode::Shadow),
            Self::On => Some(DecisionMode::On),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Point {
    StopCheck,
    WakeCheck,
    Triage,
    CommitMessage,
    Effort,
}

impl Point {
    pub fn name(self) -> &'static str {
        match self {
            Self::StopCheck => "fleet_stop_check",
            Self::WakeCheck => "fleet_wake_check",
            Self::Triage => "fleet_triage",
            Self::CommitMessage => "fleet_commit_message",
            Self::Effort => "fleet_effort",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DecisionsDefinition {
    pub stop_check: Mode,
    pub wake_check: Mode,
    pub triage: Mode,
    pub commit_message: Mode,
    pub effort: Mode,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DecisionsOverride {
    pub stop_check: Option<Mode>,
    pub wake_check: Option<Mode>,
    pub triage: Option<Mode>,
    pub commit_message: Option<Mode>,
    pub effort: Option<Mode>,
}

impl DecisionsDefinition {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn mode(&self, point: Point, worker: Option<&DecisionsOverride>) -> Mode {
        let (fleet, own) = match point {
            Point::StopCheck => (self.stop_check, worker.and_then(|w| w.stop_check)),
            Point::WakeCheck => (self.wake_check, worker.and_then(|w| w.wake_check)),
            Point::Triage => (self.triage, worker.and_then(|w| w.triage)),
            Point::CommitMessage => (self.commit_message, worker.and_then(|w| w.commit_message)),
            Point::Effort => (self.effort, worker.and_then(|w| w.effort)),
        };
        own.unwrap_or(fleet)
    }
}

impl super::super::definition::FleetDefinition {
    pub fn decision_mode(&self, point: Point, discipline: Option<&str>) -> Mode {
        let worker = discipline
            .and_then(|name| self.workers.get(name))
            .and_then(|worker| worker.decisions.as_ref());
        self.decisions.mode(point, worker)
    }

    pub fn session_decision_mode(
        &self,
        point: Point,
        seated: &super::super::seated::Seated,
        session: uuid::Uuid,
    ) -> Mode {
        let discipline = seated
            .task_of(session)
            .and_then(|task| self.tasks.get(task))
            .map(|task| task.discipline.as_str());
        self.decision_mode(point, discipline)
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
