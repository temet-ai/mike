use std::collections::BTreeMap;

use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::llm::OpenRouterClient;
use crate::llm::decisions::Answer;
use crate::llm::decisions::record::DecisionMode;
use crate::settings::SettingsOverlay;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;

const TRUST: &str = "Do you trust the files in this folder?\n1. Yes, proceed\n2. No, exit\n";

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

fn reply(key: &str) -> Value {
    json!({
        "model": "typesafe/jev-1.14",
        "answers": {WHICH: {"type": "choice", "choice": key, "probabilities": {key: 0.9}}},
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

#[test]
fn compose_a_known_dialog_gives_its_name_and_probability() {
    assert_eq!(
        compose(&picked("folder_trust", 0.9)),
        Some(("the folder trust dialog", 0.9))
    );
}

#[test]
fn compose_unknown_gives_no_name() {
    assert_eq!(compose(&picked("unknown", 0.9)), None);
}

#[test]
fn compose_a_failed_call_gives_no_name() {
    assert_eq!(compose(&Asked::failed_with("timeout")), None);
}

#[test]
fn state_keeps_the_end_of_a_long_screen() {
    let screen = format!("{}END", "x".repeat(4000));
    let state = state(&screen);
    let tail = state["screen_tail"].as_str().unwrap_or_default();
    assert_eq!(tail.chars().count(), SCREEN_CHARS);
    assert!(tail.ends_with("END"));
}

#[tokio::test]
async fn dialog_with_the_layer_off_uses_the_screen_rule() {
    assert_eq!(
        dialog(None, TRUST).await.as_deref(),
        Some("the folder trust dialog")
    );
}

#[tokio::test]
async fn dialog_on_an_answered_choice_names_the_dialog_jev_picked() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let server = stub(200, reply("permission_prompt")).await;
    let decider = decider(&server, &persist)?;
    let found = dialog(Some(&decider), "Allow this command?\n1. Yes\n2. No").await;
    assert_eq!(found.as_deref(), Some("a tool permission prompt"));
    persist.pool().close().await;
    db.cleanup().await
}

#[tokio::test]
async fn dialog_with_a_failing_client_falls_back_to_the_screen_rule() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let server = stub(500, json!({})).await;
    let decider = decider(&server, &persist)?;
    let found = dialog(Some(&decider), TRUST).await;
    assert_eq!(found.as_deref(), Some("the folder trust dialog"));
    persist.pool().close().await;
    db.cleanup().await
}
