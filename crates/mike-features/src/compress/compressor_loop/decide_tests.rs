use std::collections::HashMap;

use serde_json::{Value, json};

use super::decide::{Costs, Settings, decide};
use super::types::{Decision, FILE_SHARE};

fn settings(share: i64) -> Settings {
    Settings(HashMap::from([(FILE_SHARE.to_string(), json!(share))]))
}

fn run(forward: f64, control: f64, p: f64, qualified: bool, share: i64) -> Vec<Decision> {
    let (f, c) = (vec![forward; 40], vec![control; 40]);
    let costs = Costs {
        forward: &f,
        control: &c,
        p,
    };
    decide(&costs, &settings(share), qualified)
}

#[test]
fn decide_dearer_forward_arm_never_stops_forwarding() {
    assert!(run(200.0, 100.0, 0.01, true, 50).is_empty());
    assert!(run(200.0, 100.0, 0.01, false, 50).is_empty());
}

#[test]
fn decide_lower_cost_cannot_widen_without_qualifying_evidence() {
    assert!(run(80.0, 100.0, 0.01, false, 50).is_empty());
}

#[test]
fn decide_lower_cost_widens_once_the_evidence_qualifies() {
    let decisions = run(80.0, 100.0, 0.01, true, 50);
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].action, "widen");
    assert_eq!(decisions[0].value, json!(90));
    assert_eq!(
        decisions[0].why,
        "forward arm costs less per turn (p=0.010)"
    );
}

#[test]
fn decide_unclear_difference_changes_nothing() {
    assert!(run(80.0, 100.0, 0.4, true, 50).is_empty());
}

#[test]
fn decide_share_already_wide_is_not_widened_again() {
    assert!(run(80.0, 100.0, 0.01, true, 90).is_empty());
}

#[test]
fn decide_settings_read_numbers_only() {
    let mut odd = settings(50);
    odd.0
        .insert(FILE_SHARE.to_string(), Value::String("x".into()));
    let (f, c) = (vec![80.0; 40], vec![100.0; 40]);
    let costs = Costs {
        forward: &f,
        control: &c,
        p: 0.01,
    };
    assert_eq!(decide(&costs, &odd, true).len(), 1);
}
