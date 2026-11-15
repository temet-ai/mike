use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::label_prod::ProdAnswers;
use crate::llm::OpenRouterClient;
use crate::llm::decisions::point::Decider;
use crate::llm::decisions::record::DecisionMode;
use crate::llm::decisions::record::DecisionSubject;
use crate::settings::SettingsOverlay;
use crate::steerer::prod_point::{self, POINT};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::steering::guard::GuardLabelCandidate;
use crate::store::test_support::TestDb;

const SHORT_WITH_CONTENT: &str = "no, use the other";
const FAILING_SHORT: &str = "no, the second one";
const FAILING_LONG: &str = "please carry on with the earlier plan now";
const SHARED_MESSAGE: &str = "yes, go with the first";
const LONG_NUDGE: &str = "please keep going with whatever you were doing before";

fn candidate(turn: i64, text: Option<&str>) -> GuardLabelCandidate {
    GuardLabelCandidate {
        uuid: uuid::Uuid::new_v4(),
        operator_turn_id: turn,
        session_id: 1,
        question: None,
        trigger_point: None,
        status: "resolved_correction".to_owned(),
        dispatched_at: chrono::Utc::now(),
        verdict_json: None,
        is_last: true,
        next_state: None,
        next_genuine_request_id: Some(5),
        next_genuine_after_secs: Some(10),
        next_genuine_text: text.map(str::to_owned),
        reaction: None,
        live_verdict_id: None,
        final_assistant_text: None,
        user_prompt: None,
    }
}

fn answered(p: f64) -> Value {
    json!({
        "model": "typesafe/jev-1.14",
        "answers": {"only_nudge": {"type": "noul", "noul": p}},
        "usage": {"input_tokens": 10, "output_tokens": 1, "cost": 0.00001}
    })
}

async fn stub(status: u16, body: Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/alpha/decisions"))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    server
}

fn decider(server: &MockServer, persist: &PersistHandle) -> anyhow::Result<Decider> {
    let client = OpenRouterClient::new(
        "sk-test".to_owned(),
        "typesafe/jev-1.14".to_owned(),
        Some(server.uri()),
        None,
    )?;
    Decider::with_mode(
        &client,
        persist,
        &SettingsOverlay::default(),
        POINT,
        DecisionMode::On,
    )
    .ok_or_else(|| anyhow::anyhow!("no decider"))
}

async fn persist()
-> anyhow::Result<Option<(crate::store::test_support::TestDbHandle, PersistHandle)>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = test_db.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    Ok(Some((db, handle)))
}

#[tokio::test]
async fn labeller_prod_with_the_point_off_uses_the_length_rule() {
    let answers = ProdAnswers::new(None);
    assert_eq!(
        answers
            .of(&candidate(9_100_001, Some(SHORT_WITH_CONTENT)))
            .await,
        Some(true)
    );
    assert_eq!(
        answers.of(&candidate(9_100_002, Some(LONG_NUDGE))).await,
        Some(false)
    );
    assert_eq!(answers.of(&candidate(9_100_003, None)).await, None);
}

#[tokio::test]
async fn labeller_prod_with_the_point_on_takes_jevs_answer_and_asks_once() -> anyhow::Result<()> {
    let Some((_db, persist)) = persist().await? else {
        return Ok(());
    };
    let server = stub(200, answered(0.1)).await;
    let answers = ProdAnswers::new(Some(decider(&server, &persist)?));
    let row = candidate(9_100_010, Some(SHORT_WITH_CONTENT));
    assert_eq!(answers.of(&row).await, Some(false));
    assert_eq!(answers.of(&row).await, Some(false));
    assert_eq!(server.received_requests().await.map_or(0, |r| r.len()), 1);

    let nudge = candidate(9_100_011, Some(LONG_NUDGE));
    let server = stub(200, answered(0.95)).await;
    let answers = ProdAnswers::new(Some(decider(&server, &persist)?));
    assert_eq!(answers.of(&nudge).await, Some(true));
    Ok(())
}

#[tokio::test]
async fn labeller_prod_with_jev_failing_uses_the_length_rule_and_stops_asking() -> anyhow::Result<()>
{
    let Some((_db, persist)) = persist().await? else {
        return Ok(());
    };
    let server = stub(500, json!({"error": "down"})).await;
    let answers = ProdAnswers::new(Some(decider(&server, &persist)?));
    assert_eq!(
        answers.of(&candidate(9_100_020, Some(FAILING_SHORT))).await,
        Some(true)
    );
    assert_eq!(
        answers.of(&candidate(9_100_021, Some(FAILING_LONG))).await,
        Some(false)
    );
    assert_eq!(server.received_requests().await.map_or(0, |r| r.len()), 1);
    Ok(())
}

#[tokio::test]
async fn labeller_and_proposer_share_one_answer_per_message_and_previous_reply()
-> anyhow::Result<()> {
    let Some((_db, persist)) = persist().await? else {
        return Ok(());
    };
    let server = stub(200, answered(0.1)).await;
    let decider = decider(&server, &persist)?;
    let subject = |turn: i64| DecisionSubject {
        session_id: Some(1),
        operator_turn_id: Some(turn),
        ..DecisionSubject::default()
    };
    let reply = Some("Do you want A or B?");
    let first =
        prod_point::detect_checked(Some(&decider), SHARED_MESSAGE, reply, subject(9_100_030)).await;
    let second =
        prod_point::detect_checked(Some(&decider), SHARED_MESSAGE, reply, subject(9_100_031)).await;
    assert!(!first.is_prod && !second.is_prod);
    assert_eq!(server.received_requests().await.map_or(0, |r| r.len()), 1);

    prod_point::detect_checked(
        Some(&decider),
        SHARED_MESSAGE,
        Some("Shall I continue?"),
        subject(9_100_032),
    )
    .await;
    assert_eq!(server.received_requests().await.map_or(0, |r| r.len()), 2);
    Ok(())
}

#[tokio::test]
async fn labeller_sends_the_previous_reply_tail_as_a_fact() -> anyhow::Result<()> {
    let Some((_db, persist)) = persist().await? else {
        return Ok(());
    };
    let server = stub(200, answered(0.1)).await;
    let answers = ProdAnswers::new(Some(decider(&server, &persist)?));
    let mut row = candidate(9_100_040, Some("sure, do the second"));
    row.final_assistant_text = Some("I can do the first or the second.".to_owned());
    answers.of(&row).await;
    let requests = server.received_requests().await.unwrap_or_default();
    let body = String::from_utf8_lossy(&requests[0].body).into_owned();
    assert!(body.contains("I can do the first or the second."));
    assert!(body.contains("previous_reply_tail"));
    Ok(())
}
