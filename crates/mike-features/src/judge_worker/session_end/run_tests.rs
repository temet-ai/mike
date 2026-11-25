use super::effective_end;
use crate::judge_worker::session_end::schema::SessionVerdictV3;
use crate::judge_worker::session_end::{EndRule, SessionEndLabel};
use crate::store::persistence::session_end::SessionEndFacts;

fn facts() -> SessionEndFacts {
    SessionEndFacts {
        session_id: 1,
        end_turn_id: 10,
        end_turn_closed_at: None,
        next_state: "none_session_settled".to_owned(),
        session_class: "operator".to_owned(),
        session_class_rule: "default".to_owned(),
        aborted: false,
        interrupted: false,
        close_cause: "idle_reap".to_owned(),
        stop_reason: Some("end_turn".to_owned()),
        ended_on: "report".to_owned(),
        genuine_turns: 3,
        prior_reactions: vec![Some("accepted".to_owned())],
        launch_seat: None,
        launch_run_id: None,
        launch_template_id: None,
        brief_path: None,
        brief_text: None,
        goal_role: None,
        all_genuine_are_brief: false,
    }
}

fn seat() -> SessionEndFacts {
    SessionEndFacts {
        session_class: "fleet_seat".to_owned(),
        session_class_rule: "launch".to_owned(),
        launch_seat: Some("code-1".to_owned()),
        launch_run_id: Some("run".to_owned()),
        all_genuine_are_brief: true,
        ..facts()
    }
}

fn frustrated() -> SessionVerdictV3 {
    SessionVerdictV3 {
        outcome: "failure".to_owned(),
        quality_score: 0.1,
        rationale: String::new(),
        overview: String::new(),
        topics: Vec::new(),
        learnings: Vec::new(),
        end_label: "closed_in_frustration".to_owned(),
        end_evidence: "stop".to_owned(),
        end_confidence: "high".to_owned(),
        failure: "it looped on the same check".to_owned(),
        failure_tags: vec!["slow-or-wasteful".to_owned()],
        severity: "major".to_owned(),
    }
}

const BOTH: [EndRule; 2] = [EndRule::FleetSeatV1, EndRule::DoneV1];

#[test]
fn effective_end_done_rule_fires_keeps_the_models_label_and_failure() {
    let (fired, end) = effective_end(Some(&facts()), &frustrated(), &BOTH);
    assert_eq!(fired, Some(EndRule::DoneV1));
    let Some(end) = end else {
        panic!("no end verdict");
    };
    assert_eq!(end.label, SessionEndLabel::ClosedInFrustration);
    assert_eq!(end.deterministic, None);
    assert_eq!(end.failure, "it looped on the same check");
}

#[test]
fn effective_end_fleet_seat_rule_still_overrides_the_model() {
    let (fired, end) = effective_end(Some(&seat()), &frustrated(), &BOTH);
    assert_eq!(fired, Some(EndRule::FleetSeatV1));
    let Some(end) = end else {
        panic!("no end verdict");
    };
    assert_eq!(end.label, SessionEndLabel::FleetSeat);
    assert_eq!(end.deterministic, Some(EndRule::FleetSeatV1));
    assert!(end.failure.is_empty());
}

#[test]
fn effective_end_unsettled_session_has_no_end_verdict() {
    let open = SessionEndFacts {
        next_state: "pending".to_owned(),
        ..facts()
    };
    assert_eq!(
        effective_end(Some(&open), &frustrated(), &BOTH),
        (None, None)
    );
}
