use std::collections::BTreeMap;

use serde_json::json;

use super::*;
use crate::launch::fleet::decisions::test_support::{
    DecisionsFixture, choice_body, jev_decider, stub_jev,
};
use crate::launch::fleet::leaves::TaskTrail;
use crate::launch::fleet::leaves::tests::Canned;
use crate::llm::decisions::Answer;
use crate::llm::decisions::record::DecisionMode;

fn dossier() -> Dossier {
    Dossier {
        problem_id: "p-1".to_owned(),
        trigger: "floor_failed".to_owned(),
        detail: "the floor failed at abc".to_owned(),
        plan_version: None,
        tasks: vec![TaskTrail {
            task: "T1".to_owned(),
            recent: (0..10).map(|n| format!("event {n}")).collect(),
        }],
        earlier_decisions: vec!["red_baseline -: record_fault".to_owned()],
        lessons: Vec::new(),
        allowed_decisions: DecisionKind::names(),
        turns: 40,
        minutes: 60,
        decisions_before_the_operator: 3,
    }
}

fn picked(key: &str, p: f64) -> Asked {
    Asked::answered(BTreeMap::from([(
        ACTION.to_owned(),
        Answer::Choice {
            choice: key.to_owned(),
            probabilities: BTreeMap::from([(key.to_owned(), p)]),
            confidence: None,
        },
    )]))
}

fn others() -> Vec<&'static str> {
    DecisionKind::names()
}

#[test]
fn questions_offer_every_decision_kind() {
    let Some(Question::Choice { criteria, .. }) = questions().remove(ACTION) else {
        panic!("a choice is asked");
    };
    assert_eq!(criteria.len(), DecisionKind::ALL.len());
    for name in DecisionKind::names() {
        assert!(criteria.contains_key(name), "{name}");
    }
}

#[test]
fn compose_a_decision_key_gives_the_kind() {
    assert_eq!(
        compose(&picked("rerun_targeted", 0.7)),
        Some((DecisionKind::RerunTargeted, 0.7))
    );
}

#[test]
fn compose_an_unknown_key_or_a_failed_call_gives_nothing() {
    assert_eq!(compose(&picked("shrug", 0.9)), None);
    assert_eq!(compose(&Asked::failed_with("timeout")), None);
}

#[test]
fn state_keeps_the_last_events_of_each_task() {
    let state = state(&dossier());
    let events = state["tasks"][0]["recent_events"].as_array();
    assert_eq!(events.map(Vec::len), Some(EVENTS_PER_TASK));
    assert_eq!(state["tasks"][0]["recent_events"][5], "event 9");
}

#[test]
fn picked_decision_names_what_it_read_so_the_ledger_accepts_it() {
    let decision = picked_decision(&dossier(), DecisionKind::InstructSeats, 0.8);
    assert!(!decision.evidence_read.is_empty());
    assert!(decision.reason.contains("80%"));
    assert_eq!(decision.question, None);
}

#[tokio::test]
async fn solve_on_a_picked_action_records_it_without_asking_the_model() -> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub_jev(200, choice_body(ACTION, "instruct_seats", &others())).await;
    let decider = jev_decider(&server, &fx.handle, POINT, DecisionMode::On)?;
    let model = Canned::new(Vec::new());
    let solver = JevSolver { model, decider };
    let decision = solver.solve(&dossier()).await?;
    assert_eq!(decision.kind, DecisionKind::InstructSeats);
    assert_eq!(solver.model.calls(), 0);
    fx.finish().await
}

#[tokio::test]
async fn solve_on_escalate_asks_the_model_only_for_the_question() -> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub_jev(200, choice_body(ACTION, "escalate", &others())).await;
    let decider = jev_decider(&server, &fx.handle, POINT, DecisionMode::On)?;
    let model = Canned::new(vec![json!({"question": "Which gate owns the parser?"})]);
    let solver = JevSolver { model, decider };
    let decision = solver.solve(&dossier()).await?;
    assert_eq!(decision.kind, DecisionKind::Escalate);
    assert_eq!(
        decision.question.as_deref(),
        Some("Which gate owns the parser?")
    );
    assert_eq!(solver.model.calls(), 1);
    fx.finish().await
}

#[tokio::test]
async fn solve_with_a_failing_client_takes_the_models_decision() -> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub_jev(500, json!({})).await;
    let decider = jev_decider(&server, &fx.handle, POINT, DecisionMode::On)?;
    let model = Canned::new(vec![json!({
        "decision": "record_fault",
        "reason": "the disk was full",
        "evidence_read": ["receipt 3"],
        "question": null
    })]);
    let solver = JevSolver { model, decider };
    let decision = solver.solve(&dossier()).await?;
    assert_eq!(decision.kind, DecisionKind::RecordFault);
    assert_eq!(decision.reason, "the disk was full");
    fx.finish().await
}
