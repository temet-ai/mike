use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::rows::keep_rows;
use super::{Candidate, Context, POINT};
use crate::llm::OpenRouterClient;
use crate::llm::decisions::point::Decider;
use crate::llm::decisions::record::DecisionMode;
use crate::settings::SettingsOverlay;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;

pub fn reply(chances: &[(i64, f64)]) -> Value {
    let answers: serde_json::Map<String, Value> = chances
        .iter()
        .map(|(id, p)| (format!("lesson_{id}"), json!({"type": "noul", "noul": p})))
        .collect();
    json!({
        "model": "typesafe/jev-1.14",
        "answers": answers,
        "usage": {"input_tokens": 10, "output_tokens": 1, "cost": 0.00001}
    })
}

pub async fn stub(status: u16, body: Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/alpha/decisions"))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    server
}

pub fn decider(server: &MockServer, persist: &PersistHandle) -> anyhow::Result<Decider> {
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

pub async fn asked_bodies(server: &MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .map(|request| String::from_utf8_lossy(&request.body).into_owned())
        .collect()
}

struct Row {
    id: i64,
    text: &'static str,
}

fn row_candidate(row: &Row) -> Candidate<'_> {
    Candidate {
        id: row.id,
        kind: "correction",
        text: row.text,
    }
}

fn rows() -> Vec<Row> {
    vec![
        Row {
            id: 1,
            text: "use scripts/cx",
        },
        Row {
            id: 2,
            text: "never cargo clean",
        },
    ]
}

fn ids(rows: &[Row]) -> Vec<i64> {
    rows.iter().map(|row| row.id).collect()
}

#[tokio::test]
async fn keep_rows_without_a_decider_returns_every_row() {
    let context = Context {
        session: "supply-rows-off",
        request_id: 1,
        text: "editing the parser",
    };
    let kept = keep_rows(None, &context, rows(), row_candidate).await;
    assert_eq!(ids(&kept), vec![1, 2]);
}

#[tokio::test]
async fn keep_rows_on_keeps_rows_at_the_floor_and_holds_the_rest() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let server = stub(200, reply(&[(1, 0.9), (2, 0.2)])).await;
    let decider = decider(&server, &persist)?;
    let session = uuid::Uuid::new_v4().to_string();
    let context = Context {
        session: &session,
        request_id: 1,
        text: "editing the parser",
    };
    let kept = keep_rows(Some(&decider), &context, rows(), row_candidate).await;
    assert_eq!(ids(&kept), vec![1]);
    assert!(super::is_held(&session, 2));
    assert!(!super::is_held(&session, 1));
    persist.pool().close().await;
    db.cleanup().await
}

#[tokio::test]
async fn keep_rows_failing_call_returns_every_row() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let server = stub(500, json!({})).await;
    let decider = decider(&server, &persist)?;
    let session = uuid::Uuid::new_v4().to_string();
    let context = Context {
        session: &session,
        request_id: 1,
        text: "editing the parser",
    };
    let kept = keep_rows(Some(&decider), &context, rows(), row_candidate).await;
    assert_eq!(ids(&kept), vec![1, 2]);
    assert!(!super::is_held(&session, 1) && !super::is_held(&session, 2));
    persist.pool().close().await;
    db.cleanup().await
}

#[tokio::test]
async fn keep_rows_does_not_ask_again_about_a_held_row() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let server = stub(200, reply(&[(1, 0.9), (2, 0.2)])).await;
    let decider = decider(&server, &persist)?;
    let session = uuid::Uuid::new_v4().to_string();
    let context = Context {
        session: &session,
        request_id: 1,
        text: "editing the parser",
    };
    keep_rows(Some(&decider), &context, rows(), row_candidate).await;
    let second = keep_rows(Some(&decider), &context, rows(), row_candidate).await;
    assert_eq!(ids(&second), vec![1]);
    let bodies = asked_bodies(&server).await;
    assert_eq!(bodies.len(), 2);
    assert!(bodies[0].contains("lesson_2"));
    assert!(!bodies[1].contains("lesson_2"));
    persist.pool().close().await;
    db.cleanup().await
}
