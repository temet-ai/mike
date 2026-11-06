use super::Question;

pub const FLEET_SEAT_CLASS: &str = "fleet_seat";
const FLEET_SEAT_REASON: &str = "session_class:fleet_seat";

pub const EXEMPT_SESSION_CLASSES: [&str; 5] = [
    FLEET_SEAT_CLASS,
    "template",
    "synthetic",
    "subagent",
    "workflow",
];

pub struct ExemptInput<'a> {
    pub agent_id: Option<&'a str>,
    pub session_class: Option<&'a str>,
    pub opened_by: Option<&'a str>,
    pub open_prompt: Option<&'a str>,
}

pub fn exemption(input: &ExemptInput<'_>) -> Option<String> {
    if input.agent_id.is_some_and(|a| !a.is_empty()) {
        return Some("agent_request".to_owned());
    }
    if let Some(class) = input
        .session_class
        .filter(|c| EXEMPT_SESSION_CLASSES.contains(c))
    {
        return Some(format!("session_class:{class}"));
    }
    crate::dossier::genuine::opened_by_something_else(input.opened_by, input.open_prompt)
        .then(|| "machine_opened_turn".to_owned())
}

pub fn fleet_seat_released(question: Question, fleet_seats: bool) -> bool {
    fleet_seats && matches!(question, Question::UnobservedDone | Question::UnbackedWait)
}

pub fn question_exemption(
    input: &ExemptInput<'_>,
    question: Question,
    fleet_seats: bool,
) -> Option<String> {
    let reason = exemption(input)?;
    let released = reason == FLEET_SEAT_REASON && fleet_seat_released(question, fleet_seats);
    (!released).then_some(reason)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::steerer::guard::Trigger;
    use crate::steerer::guard::gates::phrase_gates;
    use crate::steerer::guard::question_gates::{GateContext, done_gate, wait_gate};

    fn input<'a>(agent: Option<&'a str>, class: Option<&'a str>) -> ExemptInput<'a> {
        ExemptInput {
            agent_id: agent,
            session_class: class,
            opened_by: Some("operator"),
            open_prompt: Some("please add the retry and run the tests"),
        }
    }

    #[test]
    fn agent_id_request_is_exempt() {
        assert_eq!(
            exemption(&input(Some("a-1"), None)).as_deref(),
            Some("agent_request")
        );
    }

    #[test]
    fn fleet_seat_is_exempt() {
        assert_eq!(
            exemption(&input(None, Some("fleet_seat"))).as_deref(),
            Some("session_class:fleet_seat")
        );
    }

    #[test]
    fn a_fleet_seat_done_claim_and_wait_are_not_exempt_when_the_setting_is_on() {
        let seat = input(None, Some("fleet_seat"));
        assert_eq!(
            question_exemption(&seat, Question::UnobservedDone, true),
            None
        );
        assert_eq!(
            question_exemption(&seat, Question::UnbackedWait, true),
            None
        );
        assert_eq!(
            question_exemption(&seat, Question::UnobservedDone, false).as_deref(),
            Some("session_class:fleet_seat")
        );
    }

    #[test]
    fn a_fleet_seat_stays_exempt_from_relapse_and_handback_whatever_the_setting() {
        let seat = input(None, Some("fleet_seat"));
        for on in [true, false] {
            assert!(question_exemption(&seat, Question::Relapse, on).is_some());
            assert!(question_exemption(&seat, Question::AuthorisedHandback, on).is_some());
        }
    }

    #[test]
    fn the_fleet_seat_setting_never_releases_an_agent_request() {
        let agent = input(Some("a-1"), Some("fleet_seat"));
        assert_eq!(
            question_exemption(&agent, Question::UnobservedDone, true).as_deref(),
            Some("agent_request")
        );
    }

    #[test]
    fn template_is_exempt() {
        assert_eq!(
            exemption(&input(None, Some("template"))).as_deref(),
            Some("session_class:template")
        );
    }

    #[test]
    fn coordinator_seat_is_not_exempt() {
        assert_eq!(exemption(&input(None, Some("operator"))), None);
    }

    #[test]
    fn machine_opened_turn_is_exempt() {
        let mut i = input(None, None);
        i.opened_by = Some("mailbox");
        assert_eq!(exemption(&i).as_deref(), Some("machine_opened_turn"));
    }

    #[test]
    fn ask_user_question_ending_closes_wait_and_done() {
        let text = "I'll report back once it finishes. Everything is done.";
        let phrase = phrase_gates(text, "");
        let ctx = GateContext {
            phrase: &phrase,
            trigger: Trigger::TurnEnd,
            facts: None,
            prior: &[],
            open_turn_id: 1,
            turn_judge_live: true,
            last_tool: Some("AskUserQuestion"),
            directives: &[],
            idle_seconds: None,
            quiescence_after_secs: 90,
            ending: text,
        };
        assert!(!wait_gate(&ctx).outcome.open);
        assert!(wait_gate(&ctx).outcome.reason.starts_with("real_decision"));
        let mut plan = GateContext {
            last_tool: Some("ExitPlanMode"),
            ..ctx
        };
        assert!(done_gate(&plan).outcome.reason.starts_with("real_decision"));
        plan.last_tool = None;
        assert!(!done_gate(&plan).outcome.reason.starts_with("real_decision"));
    }
}
