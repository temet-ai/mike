use super::super::{EndRule, SessionEndLabel};
use super::{apply_rules, done_v1, fleet_seat_v1};
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
        prior_reactions: vec![Some("accepted".to_owned()), Some("accepted".to_owned())],
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
        launch_run_id: Some("models-2026".to_owned()),
        all_genuine_are_brief: true,
        ..facts()
    }
}

#[test]
fn fleet_seat_v1_fires_on_a_brief_only_seat_and_never_on_a_coordinator() {
    assert_eq!(
        fleet_seat_v1(&seat()).map(|v| v.label),
        Some(SessionEndLabel::FleetSeat)
    );
    let coordinator = SessionEndFacts {
        launch_seat: Some("coordinator".to_owned()),
        ..seat()
    };
    assert!(fleet_seat_v1(&coordinator).is_none());
    let typed = SessionEndFacts {
        all_genuine_are_brief: false,
        ..seat()
    };
    assert!(fleet_seat_v1(&typed).is_none());
    assert!(fleet_seat_v1(&facts()).is_none());
}

#[test]
fn done_v1_needs_clean_projected_reactions_and_a_composed_ending() {
    assert_eq!(
        done_v1(&facts()).map(|v| v.label),
        Some(SessionEndLabel::Done)
    );
    let corrected = SessionEndFacts {
        prior_reactions: vec![Some("corrected".to_owned())],
        ..facts()
    };
    assert!(done_v1(&corrected).is_none());
    let unjudged = SessionEndFacts {
        prior_reactions: vec![None],
        ..facts()
    };
    assert!(
        done_v1(&unjudged).is_none(),
        "a NULL reaction makes the rule not fire"
    );
    assert!(
        done_v1(&SessionEndFacts {
            aborted: true,
            ..facts()
        })
        .is_none()
    );
    assert!(
        done_v1(&SessionEndFacts {
            ended_on: "promise_or_wait".to_owned(),
            ..facts()
        })
        .is_none()
    );
    assert!(
        done_v1(&SessionEndFacts {
            stop_reason: Some("tool_use".to_owned()),
            ..facts()
        })
        .is_none()
    );
    let only_turn = SessionEndFacts {
        prior_reactions: Vec::new(),
        ..facts()
    };
    assert!(
        done_v1(&only_turn).is_some(),
        "a session whose only genuine turn is E passes vacuously"
    );
}

#[test]
fn rule_fired_is_recorded_even_when_the_rule_is_not_applied() {
    let (fired, applied) = apply_rules(&seat(), &[]);
    assert_eq!(fired, Some(EndRule::FleetSeatV1));
    assert!(applied.is_none());
    let (fired, applied) = apply_rules(&seat(), &[EndRule::FleetSeatV1, EndRule::DoneV1]);
    assert_eq!(fired, Some(EndRule::FleetSeatV1));
    assert!(applied.is_some());
}
