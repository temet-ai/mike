use std::collections::VecDeque;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use super::*;
use crate::launch::fleet::events::{EventKind, FleetEvent};
use crate::launch::fleet::floor::{FailCode, Outcome};
use crate::launch::fleet::problems::{Ledger, Trigger, open_events};

pub(super) struct Canned {
    answers: Mutex<VecDeque<Value>>,
    seen: Mutex<Vec<String>>,
}

impl Canned {
    pub(super) fn new(answers: Vec<Value>) -> Self {
        Self {
            answers: Mutex::new(answers.into()),
            seen: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn calls(&self) -> usize {
        self.seen.lock().map(|s| s.len()).unwrap_or_default()
    }

    pub(super) fn user(&self, at: usize) -> String {
        self.seen
            .lock()
            .map(|s| s.get(at).cloned().unwrap_or_default())
            .unwrap_or_default()
    }
}

#[async_trait]
impl LeafModel for Canned {
    async fn answer(&self, _system: &str, user: &str, _schema: Value) -> Result<Value, LeafError> {
        if let Ok(mut seen) = self.seen.lock() {
            seen.push(user.to_string());
        }
        self.answers
            .lock()
            .ok()
            .and_then(|mut a| a.pop_front())
            .ok_or_else(|| LeafError::Transport("no canned answer left".into()))
    }
}

fn not_strict(schema: &Value, path: &str) -> Vec<String> {
    let mut faults = Vec::new();
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        if schema.get("additionalProperties") != Some(&Value::Bool(false)) {
            faults.push(format!("{path}: additionalProperties is not false"));
        }
        let required: Vec<&str> = schema
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        for (key, value) in properties {
            if !required.contains(&key.as_str()) {
                faults.push(format!("{path}.{key}: not in required"));
            }
            faults.extend(not_strict(value, &format!("{path}.{key}")));
        }
    }
    if let Some(items) = schema.get("items") {
        faults.extend(not_strict(items, &format!("{path}[]")));
    }
    faults
}

#[test]
fn every_leaf_schema_is_strict_as_the_codex_transport_requires() {
    for (name, schema) in [
        ("judge", super::judge::schema()),
        ("checker", super::checker::schema()),
        ("solver", super::pass::schema()),
    ] {
        let faults = not_strict(&schema, name);
        assert!(faults.is_empty(), "{faults:?}");
    }
}

fn input(floor: Outcome) -> JudgeInput {
    JudgeInput {
        task: "T4".into(),
        criteria: vec![CriterionText {
            id: "C3".into(),
            text: "A report is typed".into(),
            check: "fleet_report without exercised is refused".into(),
        }],
        report: ReportFacts {
            commit: "abc123".into(),
            criteria: vec!["C3".into()],
            exercised_command: "scripts/cx nextest run".into(),
            exercised_exit: Some(0),
            exercised_output_ref: "receipt 7".into(),
            tests_changed: vec!["report_tests.rs".into()],
            unestablished: Vec::new(),
        },
        floor,
        diff: "+fn refused()".into(),
        open_obligations: Vec::new(),
        task_brief: String::new(),
        shared_with: Vec::new(),
        earlier_verdicts: Vec::new(),
    }
}

#[tokio::test]
async fn the_judge_is_told_the_round_and_what_earlier_rounds_needed() -> anyhow::Result<()> {
    let model = Canned::new(vec![answer("pass", Value::Null)]);
    let mut third = input(passed());
    third.earlier_verdicts = vec![
        EarlierVerdict {
            round: 1,
            passed: false,
            criteria: vec!["C3".into()],
            needed: "refuse a report without exercised".into(),
        },
        EarlierVerdict {
            round: 2,
            passed: false,
            criteria: vec!["C3".into()],
            needed: "name the field in the refusal".into(),
        },
    ];
    judge(&model, &third).await?;
    let seen: Value = serde_json::from_str(&model.user(0))?;
    assert_eq!(seen["round"], 3);
    assert_eq!(seen["earlier_verdicts"].as_array().map(Vec::len), Some(2));
    assert_eq!(
        seen["earlier_verdicts"][1]["needed"],
        "name the field in the refusal"
    );
    Ok(())
}

#[test]
fn a_third_fail_on_the_same_criterion_names_it_and_two_do_not() {
    let fail = |needed: &str| JudgeAnswer {
        verdict: JudgeVerdict::Fail,
        criteria: vec!["C3".into()],
        sha: "abc123".into(),
        evidence_read: vec!["diff".into()],
        needed: Some(needed.into()),
        observation: "still missing".into(),
        checks: Vec::new(),
    };
    let round = |n: usize| EarlierVerdict {
        round: n,
        passed: false,
        criteria: vec!["C3".into()],
        needed: format!("thing {n}"),
    };
    assert_eq!(
        repeatedly_failed(&fail("thing 2"), &[round(1)]),
        Vec::<String>::new()
    );
    assert_eq!(
        repeatedly_failed(&fail("thing 3"), &[round(1), round(2)]),
        vec!["C3".to_string()]
    );
    let mut passed_once = fail("thing 3");
    passed_once.verdict = JudgeVerdict::Pass;
    assert_eq!(
        repeatedly_failed(&passed_once, &[round(1), round(2)]),
        Vec::<String>::new()
    );
}

#[test]
fn earlier_rounds_group_one_verdict_per_report_in_time_order() {
    let verdict = |id: i64, note: u128, criterion: &str, passed: bool, needed: &str| FleetEvent {
        id,
        run_id: "run".into(),
        task: Some("T4".into()),
        session_uuid: None,
        kind: EventKind::VerdictRecorded,
        payload: serde_json::json!({
            "note_uuid": Uuid::from_u128(note), "criterion": criterion, "passed": passed, "needed": needed
        }),
        occurred_at: chrono::DateTime::from_timestamp(id, 0).unwrap_or_default(),
    };
    let events = [
        verdict(3, 2, "C3", false, "add the test"),
        verdict(1, 1, "C3", false, "refuse the report"),
        verdict(2, 1, "C4", true, ""),
        event(4, "T9", EventKind::VerdictRecorded),
    ];
    let rounds = earlier_rounds(&events, "T4");
    assert_eq!(rounds.len(), 2);
    assert_eq!(rounds[0].round, 1);
    assert!(!rounds[0].passed);
    assert_eq!(rounds[0].criteria, vec!["C3".to_string(), "C4".to_string()]);
    assert_eq!(rounds[0].needed, "refuse the report");
    assert_eq!(rounds[1].needed, "add the test");
}

fn passed() -> Outcome {
    Outcome::Passed {
        sha: "abc123".into(),
    }
}

fn answer(verdict: &str, needed: Value) -> Value {
    json!({
        "verdict": verdict,
        "criteria": ["C3"],
        "sha": "abc123",
        "evidence_read": ["report_tests.rs"],
        "needed": needed,
        "observation": "the refusal names the field",
    })
}

#[tokio::test]
async fn the_judge_records_a_pass_from_a_canned_answer() -> anyhow::Result<()> {
    let model = Canned::new(vec![answer("pass", Value::Null)]);
    let judged = judge(&model, &input(passed())).await?;
    assert_eq!(judged.answer.verdict, JudgeVerdict::Pass);
    let events = verdict_events("run", "T4", None, Uuid::new_v4(), &judged.answer);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].payload.kind()?, EventKind::VerdictRecorded);
    assert_eq!(events[0].payload.body()?["passed"], json!(true));
    Ok(())
}

#[tokio::test]
async fn the_judge_never_sees_the_summary() -> anyhow::Result<()> {
    let model = Canned::new(vec![answer("pass", Value::Null)]);
    judge(&model, &input(passed())).await?;
    assert!(!model.user(0).contains("summary"));
    assert!(model.user(0).contains("report_tests.rs"));
    Ok(())
}

#[tokio::test]
async fn a_failed_floor_makes_no_judge_call() -> anyhow::Result<()> {
    let floor = Outcome::Failed {
        sha: "abc123".into(),
        code: FailCode::Check,
        line: "check unit exited 1".into(),
    };
    let model = Canned::new(vec![answer("pass", Value::Null)]);
    let err = judge(&model, &input(floor)).await.err();
    assert!(matches!(err, Some(LeafError::FloorNotPassed { .. })));
    assert_eq!(model.calls(), 0);
    Ok(())
}

#[tokio::test]
async fn a_fail_without_needed_or_with_a_wrong_sha_is_refused() -> anyhow::Result<()> {
    let model = Canned::new(vec![answer("fail", Value::Null)]);
    let err = judge(&model, &input(passed())).await.err();
    assert_eq!(err, Some(LeafError::Invalid("needed is empty".into())));
    let mut wrong = answer("pass", Value::Null);
    wrong["sha"] = json!("zzz");
    let model = Canned::new(vec![wrong]);
    let err = judge(&model, &input(passed())).await.err();
    assert!(matches!(err, Some(LeafError::Invalid(m)) if m.contains("sha zzz")));
    let model = Canned::new(vec![json!({"verdict": "pass"})]);
    let err = judge(&model, &input(passed())).await.err();
    assert!(matches!(err, Some(LeafError::Invalid(m)) if m.contains("criteria")));
    Ok(())
}

#[tokio::test]
async fn the_judge_may_call_the_checker_once() -> anyhow::Result<()> {
    let mut asking = answer("check", Value::Null);
    asking["checks"] = json!([{
        "question": "does the refusal name the field?",
        "artifact": "receipt 7",
        "expected": "exercised is missing"
    }]);
    let model = Canned::new(vec![
        asking.clone(),
        json!({"observed": true, "evidence": "refused: exercised is missing"}),
        answer("fail", json!("add the exercised output reference")),
    ]);
    let judged = judge(&model, &input(passed())).await?;
    assert_eq!(model.calls(), 3);
    assert_eq!(judged.answer.verdict, JudgeVerdict::Fail);
    assert_eq!(judged.checked.len(), 1);
    assert!(model.user(2).contains("refused: exercised is missing"));
    let again = Canned::new(vec![
        asking.clone(),
        json!({"observed": false, "evidence": "not shown"}),
        asking,
    ]);
    let err = judge(&again, &input(passed())).await.err();
    assert!(matches!(err, Some(LeafError::Invalid(m)) if m.contains("allowed once")));
    Ok(())
}

#[tokio::test]
async fn the_checker_answers_a_bounded_question_from_a_canned_answer() -> anyhow::Result<()> {
    let question = CheckerQuestion {
        question: "is T4 reworking?".into(),
        artifact: "smoke after-run".into(),
        expected: "floor_failed then seat woken".into(),
    };
    let model = Canned::new(vec![
        json!({"observed": true, "evidence": "floor_failed at 02:01"}),
    ]);
    let checked = check(&model, &question).await?;
    assert!(checked.observed);
    let model = Canned::new(vec![json!({"observed": true, "evidence": " "})]);
    let err = check(&model, &question).await.err();
    assert_eq!(err, Some(LeafError::Invalid("evidence is empty".into())));
    let model = Canned::new(vec![json!({"observed": "yes", "evidence": "x"})]);
    assert!(matches!(
        check(&model, &question).await,
        Err(LeafError::Invalid(_))
    ));
    Ok(())
}

fn event(id: i64, task: &str, kind: EventKind) -> FleetEvent {
    FleetEvent {
        id,
        run_id: "run".into(),
        task: Some(task.into()),
        session_uuid: None,
        kind,
        payload: json!({"sha": "abc"}),
        occurred_at: Utc::now(),
    }
}

#[test]
fn the_solver_brief_carries_the_dossier_and_picks_its_model() -> anyhow::Result<()> {
    let opened = open_events(
        &Ledger::default(),
        "run",
        Trigger::CriterionFailedThrice,
        Some("T5"),
        "C4 failed three times",
    );
    let mut stored = Vec::new();
    for (n, new) in opened.events.iter().enumerate() {
        stored.push(FleetEvent {
            id: n as i64,
            run_id: "run".into(),
            task: new.task.clone(),
            session_uuid: None,
            kind: new.payload.kind()?,
            payload: new.payload.body()?,
            occurred_at: Utc::now(),
        });
    }
    stored.push(event(9, "T5", EventKind::FloorFailed));
    let ledger = Ledger::from_events(&stored);
    let problem = ledger
        .get(opened.problem_id)
        .ok_or_else(|| anyhow::anyhow!("problem not in ledger"))?;
    let dossier = dossier(
        problem,
        "C4 failed three times",
        &stored,
        &ledger,
        &["no sleeps".into()],
    );
    let brief = brief(&dossier);
    assert_eq!(brief.model, "opus");
    assert!(brief.text.contains(&opened.problem_id.to_string()));
    assert!(brief.text.contains("floor_failed"));
    assert!(brief.text.contains("accept_substitute_proof"));
    assert!(brief.text.contains("no sleeps"));
    let quiet = super::solver::Dossier {
        trigger: Trigger::AllSeatsIdle.as_str().into(),
        ..dossier
    };
    assert_eq!(super::solver::brief(&quiet).model, "sonnet");
    Ok(())
}
