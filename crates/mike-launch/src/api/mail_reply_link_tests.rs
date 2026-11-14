use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::llm::OpenRouterClient;
use crate::llm::decisions::Answer;
use crate::llm::decisions::record::DecisionMode;
use crate::settings::SettingsOverlay;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;

fn candidates() -> Vec<Candidate> {
    vec![
        Candidate {
            id: Uuid::from_u128(1),
            queued_at: "2026-11-14T10:00:00+00:00".to_owned(),
            body: "Can you check the parser?".to_owned(),
        },
        Candidate {
            id: Uuid::from_u128(2),
            queued_at: "2026-11-14T10:05:00+00:00".to_owned(),
            body: "Also, is the gate green?".to_owned(),
        },
    ]
}

fn picked(key: &str, p: f64) -> Asked {
    Asked::answered(BTreeMap::from([(
        WHICH.to_owned(),
        Answer::Choice {
            choice: key.to_owned(),
            probabilities: BTreeMap::from([(key.to_owned(), p)]),
            confidence: None,
        },
    )]))
}

fn reply_body(key: &str) -> serde_json::Value {
    json!({
        "model": "typesafe/jev-1.14",
        "answers": {WHICH: {"type": "choice", "choice": key, "probabilities": {key: 0.9}}},
        "usage": {"input_tokens": 10, "output_tokens": 1, "cost": 0.00001}
    })
}

async fn stub(status: u16, body: serde_json::Value) -> MockServer {
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

#[test]
fn compose_the_older_message_is_linked_not_the_newest() {
    assert_eq!(
        compose(&picked("m1", 0.9), &candidates()),
        Linked::Message {
            id: Uuid::from_u128(1),
            probability: 0.9
        }
    );
}

#[test]
fn compose_none_links_nothing() {
    assert_eq!(
        compose(&picked("none", 0.8), &candidates()),
        Linked::Nothing { probability: 0.8 }
    );
}

#[test]
fn compose_a_failed_call_or_a_stray_key_is_unanswered() {
    assert_eq!(
        compose(&Asked::failed_with("timeout"), &candidates()),
        Linked::Unanswered
    );
    assert_eq!(
        compose(&picked("m9", 0.9), &candidates()),
        Linked::Unanswered
    );
}

#[test]
fn state_lists_the_messages_oldest_first_with_the_reply_tail() {
    let state = state("Yes, the parser is fine.", &candidates());
    assert_eq!(state["reply"], "Yes, the parser is fine.");
    assert_eq!(
        state["messages_from_the_colleague_oldest_first"][0]["id"],
        "m1"
    );
    assert_eq!(
        state["messages_from_the_colleague_oldest_first"][1]["body"],
        "Also, is the gate green?"
    );
}

#[test]
fn answer_of_an_unanswered_or_inactive_decision_tells_the_mod_to_use_its_own_rule() {
    assert_eq!(answer_of(&Linked::Unanswered, true).decided_by, "off");
    let linked = Linked::Message {
        id: Uuid::from_u128(1),
        probability: 0.9,
    };
    assert_eq!(answer_of(&linked, false).decided_by, "off");
    assert_eq!(
        answer_of(&linked, true).in_reply_to,
        Some(Uuid::from_u128(1))
    );
}

#[tokio::test]
async fn link_on_an_answered_choice_names_the_message() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let server = stub(200, reply_body("m1")).await;
    let decider = decider(&server, &persist)?;
    let linked = link(&decider, "Yes, the parser is fine.", &candidates()).await;
    assert_eq!(
        linked,
        Linked::Message {
            id: Uuid::from_u128(1),
            probability: 0.9
        }
    );
    persist.pool().close().await;
    db.cleanup().await
}

#[tokio::test]
async fn link_with_a_failing_client_is_unanswered() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let server = stub(500, json!({})).await;
    let decider = decider(&server, &persist)?;
    let linked = link(&decider, "Yes.", &candidates()).await;
    assert_eq!(linked, Linked::Unanswered);
    persist.pool().close().await;
    db.cleanup().await
}
