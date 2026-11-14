use std::collections::BTreeMap;

use super::*;
use crate::llm::decisions::Answer;

fn picked(key: &str, p: f64) -> Asked {
    Asked::answered(BTreeMap::from([(
        WAKE_QUESTION.to_owned(),
        Answer::Choice {
            choice: key.to_owned(),
            probabilities: BTreeMap::from([(key.to_owned(), p)]),
            confidence: None,
        },
    )]))
}

#[test]
fn compose_not_yet_with_confidence_holds() {
    assert!(compose(&picked("not_yet", 0.9)).0);
}

#[test]
fn compose_unrelated_with_confidence_holds() {
    assert!(compose(&picked("unrelated", 0.8)).0);
}

#[test]
fn compose_wake_does_not_hold() {
    assert!(!compose(&picked("wake", 0.95)).0);
}

#[test]
fn compose_a_hesitant_not_yet_does_not_hold() {
    assert!(!compose(&picked("not_yet", 0.4)).0);
}

#[test]
fn compose_a_failed_call_does_not_hold() {
    assert!(!compose(&Asked::failed_with("timeout")).0);
}

#[test]
fn from_fleet_control_names_the_coordinator_and_acceptance_seats() {
    let coordinator = Uuid::new_v4();
    let mut def = FleetDefinition::default();
    def.coordinator.session = Some(coordinator.to_string());
    assert!(from_fleet_control(&def, coordinator));
    assert!(!from_fleet_control(&def, Uuid::new_v4()));
}

#[test]
fn state_carries_the_task_files_and_the_message_tail() {
    let mut def = FleetDefinition::default();
    def.tasks.insert(
        "T1".into(),
        crate::launch::fleet::definition::TaskDefinition {
            name: "Parser".into(),
            files: vec!["src/parser.rs".into()],
            ..Default::default()
        },
    );
    let seat = SeatContext {
        def,
        seat: "worker-T1".into(),
        task: "T1".into(),
        mode: Mode::On,
        fleet_id: 1,
    };
    let mail = Mail {
        to: Uuid::new_v4(),
        from: Some(Uuid::new_v4()),
        in_reply_to_id: None,
        body: "heads up",
        sender_label: Some("worker-T2"),
    };
    let value = state(&seat, &mail);
    assert_eq!(value["seat_task"], "Parser");
    assert_eq!(value["seat_owned_files"][0], "src/parser.rs");
    assert_eq!(value["message"], "heads up");
}
