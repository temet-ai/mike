use serde::Serialize;

use crate::store::persistence::session_end::SessionEndFacts;

use super::{EndRule, SessionEndLabel};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EndVerdict {
    pub label: SessionEndLabel,
    pub deterministic: Option<EndRule>,
    pub confidence: &'static str,
    pub evidence: String,
    pub failure: String,
    pub failure_tags: Vec<String>,
    pub severity: &'static str,
}

fn is_coordinator(f: &SessionEndFacts) -> bool {
    f.launch_seat
        .as_deref()
        .is_some_and(|s| s.starts_with("coordinator"))
        || f.goal_role.as_deref() == Some("coordinator")
}

pub fn fleet_seat_v1(f: &SessionEndFacts) -> Option<EndVerdict> {
    let seat_class = matches!(f.session_class.as_str(), "fleet_seat" | "template");
    if !seat_class || is_coordinator(f) || !f.all_genuine_are_brief {
        return None;
    }
    let seat = f.launch_seat.as_deref().unwrap_or("?");
    let run = f.launch_run_id.as_deref().unwrap_or("?");
    Some(EndVerdict {
        label: SessionEndLabel::FleetSeat,
        deterministic: Some(EndRule::FleetSeatV1),
        confidence: "high",
        evidence: format!(
            "session_class {} (rule {}), seat {seat} of run {run}; the only genuine prompt is the launch brief",
            f.session_class, f.session_class_rule
        ),
        failure: String::new(),
        failure_tags: Vec::new(),
        severity: "none",
    })
}

fn prior_reactions_clean(f: &SessionEndFacts) -> bool {
    f.prior_reactions
        .iter()
        .all(|r| r.as_deref().is_some_and(|r| r != "corrected"))
}

pub fn done_v1(f: &SessionEndFacts) -> Option<EndVerdict> {
    let ok = prior_reactions_clean(f)
        && !f.aborted
        && !f.interrupted
        && f.close_cause != "abort"
        && f.ended_on != "promise_or_wait"
        && f.stop_reason.as_deref() != Some("tool_use");
    ok.then(|| EndVerdict {
        label: SessionEndLabel::Done,
        deterministic: Some(EndRule::DoneV1),
        confidence: "medium",
        evidence: format!(
            "{} earlier genuine turn(s), none corrected; the final turn was not aborted or interrupted, ended_on {}, stop_reason {}",
            f.prior_reactions.len(),
            f.ended_on,
            f.stop_reason.as_deref().unwrap_or("none")
        ),
        failure: String::new(),
        failure_tags: Vec::new(),
        severity: "none",
    })
}

pub fn first_rule(f: &SessionEndFacts) -> Option<EndVerdict> {
    fleet_seat_v1(f).or_else(|| done_v1(f))
}

pub fn apply_rules(
    f: &SessionEndFacts,
    enabled: &[EndRule],
) -> (Option<EndRule>, Option<EndVerdict>) {
    let fired = first_rule(f);
    let rule = fired.as_ref().and_then(|v| v.deterministic);
    let applied = fired.filter(|v| v.deterministic.is_some_and(|r| enabled.contains(&r)));
    (rule, applied)
}

#[cfg(test)]
#[path = "defaults_tests.rs"]
mod tests;
