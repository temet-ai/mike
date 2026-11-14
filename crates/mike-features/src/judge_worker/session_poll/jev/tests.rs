use std::collections::BTreeMap;

use super::compose::{compose, quality_of};
use super::questions::{OUTCOME, QUALITY, QUALITY_LEVELS};
use super::state::state;
use super::{apply, synthesized};
use crate::llm::decisions::Answer;
use crate::llm::decisions::point::Asked;

fn outcome(choice: &str, p: f64) -> Answer {
    Answer::Choice {
        choice: choice.to_owned(),
        probabilities: BTreeMap::from([(choice.to_owned(), p)]),
        confidence: None,
    }
}

fn quality(best: usize) -> Answer {
    let probabilities = QUALITY_LEVELS
        .iter()
        .enumerate()
        .map(|(i, _)| (i.to_string(), if i == best { 1.0 } else { 0.0 }))
        .collect();
    Answer::Score {
        score: best as f64,
        probabilities,
        confidence: None,
    }
}

fn asked(outcome_answer: Answer, quality_answer: Answer) -> Asked {
    Asked::answered(BTreeMap::from([
        (OUTCOME.to_owned(), outcome_answer),
        (QUALITY.to_owned(), quality_answer),
    ]))
}

#[test]
fn compose_confident_success_carries_a_quality_score() {
    let d = compose(&asked(outcome("success", 0.9), quality(4))).map(|d| (d.outcome, d.quality));
    assert_eq!(d, Some(("success", Some(1.0))));
}

#[test]
fn compose_unknown_outcome_has_no_quality_score() {
    let d = compose(&asked(outcome("unknown", 0.9), quality(4)));
    assert_eq!(d.map(|d| (d.outcome, d.quality)), Some(("unknown", None)));
}

#[test]
fn compose_unsure_choice_is_unknown_with_no_quality() {
    let d = compose(&asked(outcome("failure", 0.4), quality(0)));
    assert_eq!(d.map(|d| (d.outcome, d.quality)), Some(("unknown", None)));
}

#[test]
fn compose_failed_ask_is_none_the_off_path() {
    assert!(compose(&Asked::failed_with("timeout")).is_none());
}

#[test]
fn quality_of_without_probabilities_reads_the_score_as_an_expected_level() {
    let answer = Answer::Score {
        score: 2.0,
        probabilities: BTreeMap::new(),
        confidence: None,
    };
    assert_eq!(quality_of(&answer), Some(0.5));
}

#[test]
fn quality_of_reads_index_keyed_probabilities_as_jev_returns_them() {
    let answer = Answer::Score {
        score: 2.8,
        probabilities: BTreeMap::from([
            ("0".to_owned(), 0.0),
            ("1".to_owned(), 0.0),
            ("2".to_owned(), 0.2),
            ("3".to_owned(), 0.8),
            ("4".to_owned(), 0.0),
        ]),
        confidence: Some(0.8),
    };
    let q = quality_of(&answer).unwrap_or(-1.0);
    assert!((q - 0.7).abs() < 1e-9, "{q}");
}

#[test]
fn apply_marks_unknown_quality_so_the_insert_stores_null() {
    let decision = compose(&asked(outcome("unknown", 0.9), quality(2)));
    let Some(decision) = decision else {
        panic!("no decision");
    };
    let verdict = synthesized(&decision);
    assert!(verdict.quality_unknown);
    assert_eq!(verdict.outcome, "unknown");
    let mut other = synthesized(&decision);
    other.quality_unknown = false;
    apply(&mut other, &decision);
    assert!(other.quality_unknown);
}

fn decided(outcome_name: &str) -> super::SessionJev {
    let asked = asked(outcome(outcome_name, 0.9), quality(3));
    let decision = compose(&asked);
    let Some(decision) = decision else {
        panic!("no decision");
    };
    super::SessionJev { decision, asked }
}

#[test]
fn reconcile_failed_llm_text_still_stores_the_composed_verdict() {
    use super::super::verdict_write::JudgeCall;
    let jev = decided("success");
    let call = super::reconcile(JudgeCall::Failed, Some(&jev));
    let JudgeCall::Verdict(verdict) = call else {
        panic!("expected a verdict");
    };
    assert_eq!(verdict.outcome, "success");
    assert!(verdict.overview.is_empty());
    assert!(!verdict.quality_unknown);
}

#[test]
fn reconcile_without_a_decision_leaves_the_call_as_it_was() {
    use super::super::verdict_write::JudgeCall;
    assert!(matches!(
        super::reconcile(JudgeCall::Failed, None),
        JudgeCall::Failed
    ));
    assert!(matches!(
        super::reconcile(JudgeCall::Refused, Some(&decided("success"))),
        JudgeCall::Refused
    ));
}

#[test]
fn state_keeps_the_end_of_the_session_digest() {
    let digest = format!("{}FINISHED", "a".repeat(20_000));
    let s = state("build it", &digest, 12);
    let end = s["session_end"].as_str().unwrap_or_default();
    assert!(end.ends_with("FINISHED"));
    assert_eq!(end.chars().count(), 6_000);
    assert_eq!(s["operator_turns"], 12);
}

fn judged_by_model() -> crate::judge_worker::session_end::run::SessionJudgementV3 {
    use crate::judge_worker::session_end::run::SessionJudgementV3;
    use crate::judge_worker::session_end::schema::SessionVerdictV3;
    SessionJudgementV3 {
        verdict: SessionVerdictV3 {
            outcome: "partial".to_owned(),
            quality_score: 0.4,
            rationale: "the model's reading".to_owned(),
            overview: "overview text".to_owned(),
            topics: vec!["topic".to_owned()],
            learnings: Vec::new(),
            end_label: "done".to_owned(),
            end_evidence: String::new(),
            end_confidence: "high".to_owned(),
            failure: String::new(),
            failure_tags: Vec::new(),
            severity: "none".to_owned(),
        },
        facts: None,
        rule_fired: None,
        end: None,
        input_text: "digest".to_owned(),
        digest_version: "v1".to_owned(),
        as_of: chrono::Utc::now(),
        usage: crate::llm::CallUsage::default(),
        quality_unknown: false,
    }
}

#[test]
fn live_apply_on_replaces_outcome_and_quality_but_keeps_the_models_text() {
    let Some(decision) = compose(&asked(outcome("success", 0.9), quality(4))) else {
        panic!("no decision");
    };
    let mut judged = judged_by_model();
    let before = super::live::apply_live(&mut judged, &decision);
    assert_eq!(judged.verdict.outcome, "success");
    assert!((judged.verdict.quality_score - 1.0).abs() < 1e-9);
    assert!(!judged.quality_unknown);
    assert_eq!(judged.verdict.overview, "overview text");
    assert_eq!(judged.verdict.topics, vec!["topic".to_owned()]);
    assert_eq!(before.outcome, "partial");
    assert!((before.quality - 0.4).abs() < 1e-9);
}

#[test]
fn live_apply_unknown_outcome_stores_no_quality() {
    let Some(decision) = compose(&asked(outcome("unknown", 0.9), quality(4))) else {
        panic!("no decision");
    };
    let mut judged = judged_by_model();
    super::live::apply_live(&mut judged, &decision);
    assert_eq!(judged.verdict.outcome, "unknown");
    assert!(judged.quality_unknown);
}

#[test]
fn live_failing_ask_composes_nothing_so_the_models_verdict_stands() {
    let failed = Asked::failed_with("timeout");
    assert!(compose(&failed).is_none());
    let judged = judged_by_model();
    assert_eq!(judged.verdict.outcome, "partial");
    assert!(!judged.quality_unknown);
}

#[test]
fn live_composed_record_keeps_the_models_own_numbers_beside_the_decision() {
    let Some(decision) = compose(&asked(outcome("failure", 0.8), quality(0))) else {
        panic!("no decision");
    };
    let mut judged = judged_by_model();
    let before = super::live::apply_live(&mut judged, &decision);
    let composed = super::live::composed_live(&decision, &before);
    assert_eq!(composed["outcome"], "failure");
    assert_eq!(composed["model_view"]["outcome"], "partial");
    assert!(composed["score"].as_f64().is_some());
}
