use std::collections::BTreeMap;

use chrono::Utc;

use super::*;
use crate::llm::decisions::Answer;

fn row(exit: Option<i32>, output: &str) -> TreeCheckRow {
    TreeCheckRow {
        id: 1,
        run_id: "run".into(),
        state: "s".into(),
        state_after: Some("s".into()),
        command: "make check".into(),
        started_at: Utc::now(),
        finished_at: Some(Utc::now()),
        exit_code: exit,
        output: output.into(),
        askers: Vec::new(),
        told: Vec::new(),
    }
}

fn picked(key: &str, p: f64) -> Asked {
    Asked::answered(BTreeMap::from([(
        NEXT_STEP.to_owned(),
        Answer::Choice {
            choice: key.to_owned(),
            probabilities: BTreeMap::from([(key.to_owned(), p)]),
            confidence: None,
        },
    )]))
}

#[test]
fn compose_maps_the_choice_to_an_action_with_the_probability() {
    let Some((found, p)) = compose(&picked("retry", 0.8)) else {
        panic!("a choice is composed");
    };
    assert_eq!(found.action, Action::Retry);
    assert_eq!(found.by, "jev");
    assert_eq!(p, 0.8);
}

#[test]
fn compose_a_failed_call_gives_no_triage() {
    assert_eq!(compose(&Asked::failed_with("timeout")), None);
}

#[test]
fn state_carries_the_command_and_cuts_the_output_to_its_tail() {
    let output = format!("{}END", "x".repeat(OUTPUT_TAIL_BYTES * 2));
    let value = state(&row(Some(1), &output));
    let tail = value["output_tail"].as_str().unwrap_or_default();
    assert_eq!(tail.len(), OUTPUT_TAIL_BYTES);
    assert!(tail.ends_with("END"));
    assert_eq!(value["command"], "make check");
}

#[tokio::test]
async fn triage_a_passing_check_has_none() {
    assert_eq!(triage(None, Mode::On, &row(Some(0), "ok")).await, None);
}

#[tokio::test]
async fn triage_a_rule_decides_without_a_decider() {
    let found = triage(
        None,
        Mode::On,
        &row(Some(1), "ModuleNotFoundError: No module named 'x'"),
    )
    .await;
    assert_eq!(found.map(|t| t.by), Some("rules"));
}

#[tokio::test]
async fn triage_an_unmatched_failure_with_no_decider_has_no_line() {
    assert_eq!(
        triage(None, Mode::On, &row(Some(1), "assertion failed")).await,
        None
    );
}
