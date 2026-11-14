use serde_json::json;

use super::correction::Correction;
use super::run::{SCHEMA, replay};
use super::stub_tests::{
    Mode, Stub, correction_ready, turn_judge_ready, verifier_ready, wake_ready,
};
use super::turn_judge::TurnJudge;
use super::verifier::VerifierPrecheck;
use super::wake::WakeRecheck;

#[tokio::test]
async fn correction_line_carries_request_answers_cost_and_the_right_outcome() -> anyhow::Result<()>
{
    let items = vec![correction_ready(1, "correction", Some("clarification"))];
    let stub = Stub::new(Mode::Answer(0.9));
    let replayed = replay::<Correction>(&items, &stub).await;
    let outcome = replayed
        .outcomes
        .first()
        .ok_or_else(|| anyhow::anyhow!("no outcome"))?;
    let line = &outcome.line;
    assert_eq!(line["schema"], SCHEMA);
    assert_eq!(line["point"], "correction_classifier");
    assert_eq!(line["item_id"], "1");
    assert_eq!(
        line["source_ref"],
        "decisions_replay:correction_classifier:correction_classifier_v1:1"
    );
    assert_eq!(line["label"], "correction");
    assert_eq!(line["model"], "stub-jev-served");
    assert_eq!(line["request"]["model"], "stub-jev");
    assert_eq!(
        line["request"]["state"]["operator_message"],
        "that is not what I asked"
    );
    assert_eq!(line["answers"]["corrects_prior_turn"]["noul"], 0.9);
    assert_eq!(line["cost_usd"], 0.001);
    assert_eq!(line["latency_ms"], 120);
    assert_eq!(line["composed"]["verdict"]["kind"], "correction");
    assert_eq!(line["score"], 0.9);
    assert_eq!(
        line["outcome"],
        json!({"label": "right", "source": "operator_kind"})
    );
    assert_eq!(line["agrees_current"], false);
    assert!(replayed.stopped.is_none());
    Ok(())
}

#[tokio::test]
async fn correction_low_answer_against_a_correction_label_is_wrong() -> anyhow::Result<()> {
    let items = vec![correction_ready(2, "correction", None)];
    let stub = Stub::new(Mode::Answer(0.1));
    let replayed = replay::<Correction>(&items, &stub).await;
    let line = &replayed
        .outcomes
        .first()
        .ok_or_else(|| anyhow::anyhow!("no outcome"))?
        .line;
    assert_eq!(line["outcome"]["label"], "wrong");
    assert!(line["agrees_current"].is_null());
    Ok(())
}

#[tokio::test]
async fn failing_jev_writes_an_error_line_without_composed_or_outcome() -> anyhow::Result<()> {
    let items = vec![correction_ready(3, "clarification", None)];
    let stub = Stub::new(Mode::Fail);
    let replayed = replay::<Correction>(&items, &stub).await;
    let outcome = replayed
        .outcomes
        .first()
        .ok_or_else(|| anyhow::anyhow!("no outcome"))?;
    assert!(outcome.failed);
    assert_eq!(outcome.line["error"], "timeout");
    assert!(outcome.line["composed"].is_null());
    assert!(outcome.line["outcome"].is_null());
    assert!(outcome.line["answers"].is_null());
    assert!(outcome.composed.is_none());
    Ok(())
}

#[tokio::test]
async fn a_stop_reply_ends_the_run_before_the_next_item() {
    let items = vec![
        correction_ready(1, "correction", None),
        correction_ready(2, "correction", None),
    ];
    let stub = Stub::new(Mode::Stop);
    let replayed = replay::<Correction>(&items, &stub).await;
    assert!(replayed.outcomes.is_empty());
    assert_eq!(replayed.stopped.as_deref(), Some("daily ceiling"));
    assert_eq!(stub.calls(), 1);
}

#[tokio::test]
async fn ten_failures_in_a_row_end_the_run() {
    let items: Vec<_> = (0..12)
        .map(|i| correction_ready(i, "correction", None))
        .collect();
    let stub = Stub::new(Mode::Fail);
    let replayed = replay::<Correction>(&items, &stub).await;
    assert_eq!(replayed.outcomes.len(), 10);
    assert_eq!(stub.calls(), 10);
    assert!(replayed.stopped.is_some());
}

#[tokio::test]
async fn wake_recheck_skip_against_a_wrong_fire_is_right() -> anyhow::Result<()> {
    let stub = Stub::new(Mode::Pick(&[
        ("concern_still_open", 0.1),
        ("done_since_the_note", 0.9),
    ]));
    let items = vec![wake_ready("wrong")];
    let replayed = replay::<WakeRecheck>(&items, &stub).await;
    let outcome = replayed
        .outcomes
        .first()
        .ok_or_else(|| anyhow::anyhow!("no outcome"))?;
    assert_eq!(outcome.line["composed"]["verdict"]["skip"], true);
    assert_eq!(outcome.line["outcome"]["label"], "right");
    assert_eq!(
        outcome.line["request"]["state"]["question"],
        "unbacked_wait"
    );
    assert_eq!(outcome.questions.len(), 2);
    Ok(())
}

#[tokio::test]
async fn verifier_precheck_send_against_a_wrong_fire_is_wrong() -> anyhow::Result<()> {
    let stub = Stub::new(Mode::Answer(0.9));
    let items = vec![verifier_ready("wrong", None)];
    let replayed = replay::<VerifierPrecheck>(&items, &stub).await;
    let outcome = replayed
        .outcomes
        .first()
        .ok_or_else(|| anyhow::anyhow!("no outcome"))?;
    assert_eq!(outcome.line["composed"]["verdict"]["send"], true);
    assert_eq!(outcome.line["outcome"]["label"], "wrong");
    assert_eq!(
        outcome.line["request"]["state"]["nudge"],
        "You did not run the tests."
    );
    let positives: Vec<Option<bool>> = outcome.questions.iter().map(|(_, s)| s.positive).collect();
    assert_eq!(positives, vec![Some(false), Some(true), Some(true)]);
    Ok(())
}

#[tokio::test]
async fn turn_judge_with_a_next_message_asks_the_reaction_questions() -> anyhow::Result<()> {
    let stub = Stub::new(Mode::Answer(0.9));
    let items = vec![turn_judge_ready(
        "corrected",
        Some("no, that is still broken"),
    )];
    let replayed = replay::<TurnJudge>(&items, &stub).await;
    let outcome = replayed
        .outcomes
        .first()
        .ok_or_else(|| anyhow::anyhow!("no outcome"))?;
    let questions = &outcome.line["request"]["questions"];
    assert!(questions["operator_corrected"].is_object());
    assert_eq!(
        outcome.line["request"]["state"]["operator_next_message"],
        "no, that is still broken"
    );
    assert_eq!(outcome.line["composed"]["verdict"]["label"], "corrected");
    assert_eq!(outcome.line["outcome"]["label"], "right");
    assert_eq!(outcome.line["agrees_current"], false);
    let corrected = outcome
        .questions
        .iter()
        .find(|(q, _)| q == "operator_corrected")
        .map(|(_, s)| s.positive);
    assert_eq!(corrected, Some(Some(true)));
    Ok(())
}

#[tokio::test]
async fn turn_judge_without_a_next_message_leaves_the_reaction_questions_out() -> anyhow::Result<()>
{
    let stub = Stub::new(Mode::Answer(0.2));
    let items = vec![turn_judge_ready("accepted", None)];
    let replayed = replay::<TurnJudge>(&items, &stub).await;
    let outcome = replayed
        .outcomes
        .first()
        .ok_or_else(|| anyhow::anyhow!("no outcome"))?;
    assert!(outcome.line["request"]["questions"]["operator_corrected"].is_null());
    assert!(outcome.line["request"]["state"]["operator_next_message"].is_null());
    Ok(())
}
