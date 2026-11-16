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
            exercised_output: String::new(),
            check_tests: Vec::new(),
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

fn with(met: (f64, f64), extra: &[(&str, f64)]) -> Asked {
    let mut answers = BTreeMap::new();
    let all = [("criterion_C1", met.0), ("criterion_C2", met.1)];
    for (key, p) in all.iter().chain(extra) {
        answers.insert(
            (*key).to_owned(),
            Answer::Noul {
                noul: *p,
                confidence: None,
            },
        );
    }
    Asked::answered(answers)
}

#[test]
fn compose_a_lean_met_by_the_criterions_own_green_check_passes() {
    let asked = with(
        (0.9, 0.6),
        &[("runs_check_C1", 0.9), ("runs_check_C2", 0.9)],
    );
    assert_eq!(
        compose(&asked, &input(Some(0), &[])),
        Composed::Pass { weakest: 0.6 }
    );
}

#[test]
fn compose_a_lean_met_by_another_command_asks_for_a_check() {
    let asked = with(
        (0.9, 0.6),
        &[("runs_check_C1", 0.9), ("runs_check_C2", 0.2)],
    );
    assert_eq!(compose(&asked, &input(Some(0), &[])), Composed::Check);
}

#[test]
fn compose_a_caveat_no_criterion_needs_still_passes() {
    let asked = with((0.9, 0.9), &[("caveat_C1", 0.1), ("caveat_C2", 0.2)]);
    assert_eq!(
        compose(&asked, &input(Some(0), &["an estimate the check allows"])),
        Composed::Pass { weakest: 0.9 }
    );
}

#[test]
fn compose_a_caveat_a_criterion_needs_asks_for_a_check() {
    let asked = with((0.9, 0.9), &[("caveat_C1", 0.1), ("caveat_C2", 0.8)]);
    assert_eq!(
        compose(&asked, &input(Some(0), &["C2 not seen live"])),
        Composed::Check
    );
}

#[test]
fn questions_ask_about_caveats_only_when_the_report_lists_some() {
    let plain = questions(&[criterion("C1")], &input(Some(0), &[]).report);
    assert!(plain.contains_key("runs_check_C1"));
    assert!(!plain.contains_key("caveat_C1"));
    let caveated = questions(&[criterion("C1")], &input(Some(0), &["x"]).report);
    assert!(caveated.contains_key("caveat_C1"));
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

#[test]
fn state_carries_the_task_brief_and_the_tasks_that_share_its_criteria() {
    let mut shared = input(Some(0), &[]);
    shared.task_brief = "C1: the shared types only".to_owned();
    shared.shared_with = vec![super::super::judge::SharedTask {
        task: "T3".to_owned(),
        name: "CLI".to_owned(),
        criteria: vec!["C1".to_owned()],
    }];
    let state = state(&shared);
    assert_eq!(state["task_brief"], "C1: the shared types only");
    assert_eq!(state["shared_with"][0]["task"], "T3");
}

#[test]
fn state_never_shows_jev_its_own_earlier_answers_the_round_or_the_floor_sha() {
    let mut third = input(Some(0), &[]);
    third.earlier_verdicts = vec![super::super::judge::EarlierVerdict {
        round: 1,
        passed: false,
        criteria: vec!["C1".to_owned()],
        needed: "show C1".to_owned(),
    }];
    let state = state(&third);
    for field in ["earlier_verdicts", "round", "floor_passed_at"] {
        assert!(state.get(field).is_none(), "{field}");
    }
    assert_eq!(state["task"], "T1");
}

#[test]
fn the_question_asks_for_this_tasks_part_of_a_criterion() {
    let asked = questions(&[criterion("C1")], &input(Some(0), &[]).report);
    let text = serde_json::to_string(&asked["criterion_C1"]).unwrap_or_default();
    assert!(text.contains("this task's part of criterion C1"), "{text}");
}

#[tokio::test]
async fn jev_judge_on_answers_that_show_every_criterion_passes() -> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub_jev(
        200,
        noul_body(&[
            ("criterion_C1", 0.9),
            ("criterion_C2", 0.85),
            ("runs_check_C1", 0.9),
            ("runs_check_C2", 0.9),
        ]),
    )
    .await;
    let decider = jev_decider(&server, &fx.handle, POINT, DecisionMode::On)?;
    let judged = jev_judge(&decider, &input(Some(0), &[]), uuid::Uuid::new_v4()).await;
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
    let note = uuid::Uuid::new_v4();
    assert_eq!(jev_judge(&decider, &input(Some(0), &[]), note).await, None);
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
    assert_eq!(
        jev_judge(&decider, &failed, uuid::Uuid::new_v4()).await,
        None
    );
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    fx.finish().await
}
