use std::collections::BTreeMap;

use serde_json::json;

use super::*;
use crate::launch::fleet::decisions::test_support::{
    DecisionsFixture, jev_decider, noul_body, stub_jev,
};
use crate::launch::fleet::floor::{FailCode, Outcome};
use crate::launch::fleet::leaves::ReportFacts;
use crate::llm::decisions::Answer;
use crate::llm::decisions::record::DecisionMode;

fn criterion(id: &str) -> CriterionText {
    CriterionText {
        id: id.to_owned(),
        text: format!("{id} holds"),
        check: "make test".to_owned(),
    }
}

fn input(exit: Option<i32>, unestablished: &[&str]) -> JudgeInput {
    JudgeInput {
        task: "T1".to_owned(),
        criteria: vec![criterion("C1"), criterion("C2")],
        report: ReportFacts {
            commit: "abc123".to_owned(),
            criteria: vec!["C1".to_owned(), "C2".to_owned()],
            exercised_command: "make test".to_owned(),
            exercised_exit: exit,
            exercised_output_ref: "receipt 3".to_owned(),
            tests_changed: vec!["a_test.rs".to_owned()],
            unestablished: unestablished.iter().map(|s| (*s).to_owned()).collect(),
        },
        floor: Outcome::Passed {
            sha: "abc123".to_owned(),
        },
        diff: "+fn a() {}".to_owned(),
        open_obligations: Vec::new(),
        task_brief: String::new(),
        shared_with: Vec::new(),
        earlier_verdicts: Vec::new(),
    }
}

fn answers(c1: f64, c2: f64) -> Asked {
    let noul = |p: f64| Answer::Noul {
        noul: p,
        confidence: None,
    };
    Asked::answered(BTreeMap::from([
        ("criterion_C1".to_owned(), noul(c1)),
        ("criterion_C2".to_owned(), noul(c2)),
    ]))
}

#[test]
fn compose_every_criterion_shown_met_passes() {
    assert_eq!(
        compose(&answers(0.9, 0.8), &input(Some(0), &[])),
        Composed::Pass { weakest: 0.8 }
    );
}

#[test]
fn compose_one_criterion_not_shown_fails_with_that_criterion() {
    let Composed::Fail { unmet } = compose(&answers(0.9, 0.1), &input(Some(0), &[])) else {
        panic!("a failure is composed");
    };
    assert_eq!(unmet, vec![("C2".to_owned(), 0.1)]);
}

#[test]
fn compose_a_middling_answer_asks_for_a_check() {
    assert_eq!(
        compose(&answers(0.9, 0.5), &input(Some(0), &[])),
        Composed::Check
    );
}

#[test]
fn compose_a_pass_with_unestablished_claims_or_a_red_exercise_asks_for_a_check() {
    assert_eq!(
        compose(&answers(0.9, 0.9), &input(Some(0), &["the live deploy"])),
        Composed::Check
    );
    assert_eq!(
        compose(&answers(0.9, 0.9), &input(Some(1), &[])),
        Composed::Check
    );
}

#[test]
fn compose_a_failed_call_asks_for_a_check() {
    assert_eq!(
        compose(&Asked::failed_with("timeout"), &input(Some(0), &[])),
        Composed::Check
    );
}

#[test]
fn answer_of_a_failure_names_what_is_needed_so_the_judge_validation_accepts_it() {
    let input = input(Some(0), &[]);
    let composed = Composed::Fail {
        unmet: vec![("C2".to_owned(), 0.1)],
    };
    let Some(answer) = answer_of(&composed, &input) else {
        panic!("an answer is composed");
    };
    assert_eq!(answer.verdict, JudgeVerdict::Fail);
    assert_eq!(answer.criteria, vec!["C2".to_owned()]);
    assert_eq!(answer.sha, "abc123");
    assert!(answer.needed.is_some_and(|n| n.contains("C2")));
    assert!(!answer.evidence_read.is_empty());
}

#[test]
fn answer_of_a_check_composes_nothing() {
    assert_eq!(answer_of(&Composed::Check, &input(Some(0), &[])), None);
}

#[test]
fn state_cuts_a_long_diff_between_its_head_and_its_tail() {
    let mut long = input(Some(0), &[]);
    long.diff = format!("{}{}", "a".repeat(5000), "z".repeat(5000));
    let state = state(&long);
    let diff = state["diff"].as_str().unwrap_or_default();
    assert!(diff.starts_with('a') && diff.ends_with('z'));
    assert!(diff.contains("diff cut here"));
}

#[tokio::test]
async fn jev_judge_on_answers_that_show_every_criterion_passes() -> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub_jev(
        200,
        noul_body(&[("criterion_C1", 0.9), ("criterion_C2", 0.85)]),
    )
    .await;
    let decider = jev_decider(&server, &fx.handle, POINT, DecisionMode::On)?;
    let judged = jev_judge(&decider, &input(Some(0), &[])).await;
    assert_eq!(judged.map(|j| j.answer.verdict), Some(JudgeVerdict::Pass));
    fx.finish().await
}

#[tokio::test]
async fn jev_judge_with_a_failing_client_leaves_the_judgement_to_the_model() -> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub_jev(500, json!({})).await;
    let decider = jev_decider(&server, &fx.handle, POINT, DecisionMode::On)?;
    assert_eq!(jev_judge(&decider, &input(Some(0), &[])).await, None);
    fx.finish().await
}

#[tokio::test]
async fn jev_judge_below_the_floor_asks_nothing() -> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub_jev(
        200,
        noul_body(&[("criterion_C1", 0.9), ("criterion_C2", 0.9)]),
    )
    .await;
    let decider = jev_decider(&server, &fx.handle, POINT, DecisionMode::On)?;
    let mut failed = input(Some(0), &[]);
    failed.floor = Outcome::Failed {
        sha: "abc123".to_owned(),
        code: FailCode::Check,
        line: "make test failed".to_owned(),
    };
    assert_eq!(jev_judge(&decider, &failed).await, None);
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    fx.finish().await
}
