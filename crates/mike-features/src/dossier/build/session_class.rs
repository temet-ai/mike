use std::sync::LazyLock;

use regex::Regex;

pub const SESSION_CLASS_VERSION: &str = "session-class-v2";

static PROBE: LazyLock<Regex> = LazyLock::new(|| {
    match Regex::new(
        r"(?i)Reply with (the single word|exactly|only)|Please greet me|disposable test session|verifying automated steering|^Say (hello|hi)\b",
    ) {
        Ok(re) => re,
        Err(e) => panic!("invalid session class probe pattern: {e}"),
    }
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionClass {
    Operator,
    FleetSeat,
    Template,
    Subagent,
    Workflow,
    Synthetic,
}

impl SessionClass {
    pub fn code(self) -> &'static str {
        match self {
            SessionClass::Operator => "operator",
            SessionClass::FleetSeat => "fleet_seat",
            SessionClass::Template => "template",
            SessionClass::Subagent => "subagent",
            SessionClass::Workflow => "workflow",
            SessionClass::Synthetic => "synthetic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct LaunchFacts {
    pub launch_id: i64,
    pub run_id: Option<String>,
    pub seat: Option<String>,
    pub template_id: Option<i64>,
    pub fork: bool,
}

#[derive(Debug, Clone, Default)]
pub struct LaneSummary {
    pub primary_without_agent_id: i64,
    pub agent_id_requests: i64,
    pub workflow_agent_requests: i64,
}

pub const SHORT_PROMPT_CHARS: usize = 160;

#[derive(Debug, Clone, Default)]
pub struct SessionClassInput {
    pub probe_verdict: Option<bool>,
    pub is_template_session: bool,
    pub launches: Vec<LaunchFacts>,
    pub project_path: Option<String>,
    pub first_prompt: Option<String>,
    pub lanes: LaneSummary,
    pub role: Option<String>,
    pub later_genuine_turn_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassEvidence {
    pub rule: &'static str,
    pub launch_id: Option<i64>,
    pub run_id: Option<String>,
    pub seat: Option<String>,
    pub template_id: Option<i64>,
    pub fork: bool,
    pub role: Option<String>,
    pub later_genuine_turn_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassResult {
    pub class: SessionClass,
    pub rule: &'static str,
    pub evidence: ClassEvidence,
}

fn evidence(
    input: &SessionClassInput,
    rule: &'static str,
    launch: Option<&LaunchFacts>,
) -> ClassEvidence {
    ClassEvidence {
        rule,
        launch_id: launch.map(|launch| launch.launch_id),
        run_id: launch.and_then(|launch| launch.run_id.clone()),
        seat: launch.and_then(|launch| launch.seat.clone()),
        template_id: launch.and_then(|launch| launch.template_id),
        fork: launch.is_some_and(|launch| launch.fork),
        role: input.role.clone(),
        later_genuine_turn_id: input.later_genuine_turn_id,
    }
}

fn result(
    class: SessionClass,
    rule: &'static str,
    input: &SessionClassInput,
    launch: Option<&LaunchFacts>,
) -> ClassResult {
    ClassResult {
        class,
        rule,
        evidence: evidence(input, rule, launch),
    }
}

fn is_template_launch(launch: &LaunchFacts) -> bool {
    launch
        .seat
        .as_deref()
        .is_some_and(|seat| seat.starts_with("template"))
}

fn is_seat_launch(launch: &LaunchFacts) -> bool {
    launch.run_id.is_some()
        && !launch
            .seat
            .as_deref()
            .is_some_and(|seat| seat.starts_with("coordinator"))
}

fn is_synthetic_path(path: &str) -> bool {
    path.starts_with("/tmp/") || path.contains("/scratchpad")
}

fn synthetic_rule(input: &SessionClassInput) -> Option<&'static str> {
    if input.project_path.as_deref().is_some_and(is_synthetic_path) {
        return Some("path");
    }
    let matched = input
        .first_prompt
        .as_deref()
        .is_some_and(|prompt| PROBE.is_match(prompt.trim_start()));
    match (matched, input.probe_verdict) {
        (true, Some(false)) => None,
        (true, _) => Some("probe"),
        (false, Some(true)) => Some("jev_probe"),
        (false, _) => None,
    }
}

pub fn probe_candidate(input: &SessionClassInput) -> Option<crate::dossier::probe_point::Reason> {
    use crate::dossier::probe_point::Reason;
    let prompt = input.first_prompt.as_deref()?.trim_start();
    let seat_or_template = input.is_template_session
        || input
            .launches
            .iter()
            .any(|l| is_template_launch(l) || is_seat_launch(l));
    let synthetic_path = input.project_path.as_deref().is_some_and(is_synthetic_path);
    if seat_or_template || synthetic_path || input.lanes.primary_without_agent_id == 0 {
        return None;
    }
    if PROBE.is_match(prompt) {
        return Some(Reason::RegexMatch);
    }
    (input.later_genuine_turn_id.is_none() && prompt.chars().count() <= SHORT_PROMPT_CHARS)
        .then_some(Reason::ShortSingleTurn)
}

fn lane_class(lanes: &LaneSummary) -> Option<(SessionClass, &'static str)> {
    if lanes.primary_without_agent_id > 0 || lanes.agent_id_requests == 0 {
        return None;
    }
    if lanes.workflow_agent_requests == lanes.agent_id_requests {
        Some((SessionClass::Workflow, "workflow"))
    } else {
        Some((SessionClass::Subagent, "subagent"))
    }
}

pub fn classify(input: &SessionClassInput) -> ClassResult {
    if input.is_template_session {
        return result(SessionClass::Template, "template", input, None);
    }
    if let Some(launch) = input.launches.iter().find(|l| is_template_launch(l)) {
        return result(SessionClass::Template, "template", input, Some(launch));
    }
    if let Some(launch) = input.launches.iter().find(|l| is_seat_launch(l)) {
        return result(SessionClass::FleetSeat, "launch", input, Some(launch));
    }
    if let Some(rule) = synthetic_rule(input) {
        return result(SessionClass::Synthetic, rule, input, None);
    }
    if let Some((class, rule)) = lane_class(&input.lanes) {
        return result(class, rule, input, None);
    }
    result(SessionClass::Operator, "default", input, None)
}
