use crate::store::persistence::web_repo::WebPersist as _;
use std::sync::Arc;

use axum::body::Bytes;
use axum::http::{HeaderMap, StatusCode};
use chrono::{Duration, Utc};
use serde_json::{Value, json};

use super::Conversation;
use super::fakes_tests::{Db, FakeEngine, FakeSteers, TOKEN, bearer, operator_token};
use super::steer_apply::Steers;
use super::worker::tick;
use crate::api::wright::conversation::{MessagesQuery, list_messages_with, undo_steer_with};

const REMEDY: &str = "remedy:judge_mail";

fn pause_answer() -> Value {
    let until = (Utc::now() + Duration::hours(8)).to_rfc3339();
    json!({
        "body": "I paused the judge mail fix until tonight.",
        "links": [{"kind": "problem", "id": 999_999}, {"kind": "finding", "id": 4}, {"kind": "deploy", "id": 1}],
        "steer": {"kind": "pause_remedy", "name": "judge_mail", "until": until},
    })
}

async fn undo(
    convo: &Conversation,
    headers: &HeaderMap,
    id: i64,
    version: i64,
) -> anyhow::Result<(StatusCode, Value)> {
    let token = operator_token()?;
    let bytes = Bytes::from(serde_json::to_vec(&json!({"version": version}))?);
    let response = match undo_steer_with(convo, Some(&token), headers, id, &bytes).await {
        Ok(response) | Err(response) => response,
    };
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    Ok((
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    ))
}

async fn last_steer(db: &Db) -> anyhow::Result<Value> {
    let query: MessagesQuery = serde_json::from_value(json!({}))?;
    let listed = list_messages_with(&db.handle, &query)
        .await
        .map_err(|response| anyhow::anyhow!("listing failed with {}", response.status()))?;
    listed["messages"]
        .as_array()
        .and_then(|messages| messages.iter().rev().find(|m| m["author"] == "wright"))
        .map(|m| m["steer"].clone())
        .ok_or_else(|| anyhow::anyhow!("no Wright message in {listed}"))
}

async fn answered_with(
    db: &Db,
    answer: Value,
    steers: Arc<FakeSteers>,
) -> anyhow::Result<(Conversation, Value)> {
    db.ask(
        &format!("steer-{}", uuid::Uuid::new_v4()),
        "leave the judge alone today",
    )
    .await?;
    let convo = db.convo(FakeEngine::answering(vec![Ok(answer)]), steers);
    tick(&convo).await?;
    Ok((convo, last_steer(db).await?))
}

#[tokio::test]
async fn steer_applies_shows_with_an_undo_and_undoes() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let steers = Arc::new(FakeSteers::default());
    let (convo, steer) = answered_with(&db, pause_answer(), steers.clone()).await?;
    assert_eq!(steer["can_undo"], true, "{steer}");
    assert_eq!(steer["change"]["kind"], "pause_remedy");
    assert_eq!(steers.state(REMEDY)["paused"], true);
    let links =
        sqlx::query_scalar::<_, Value>("SELECT links FROM wright.messages WHERE author = 'wright'")
            .fetch_one(db.handle.pool())
            .await?;
    assert_eq!(links, json!([{"kind": "finding", "id": 4}]));

    let id = steer["id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("no steer id in {steer}"))?;
    let (status, _) = undo(&convo, &HeaderMap::new(), id, 1).await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, undone) = undo(&convo, &bearer(TOKEN)?, id, 1).await?;
    assert_eq!(status, StatusCode::OK, "{undone}");
    assert_eq!(undone["steer"]["can_undo"], false);
    assert!(undone["steer"]["undone_at"].is_string());
    assert_eq!(steers.state(REMEDY), json!({"paused": false}));
    let (status, again) = undo(&convo, &bearer(TOKEN)?, id, 2).await?;
    assert_eq!(status, StatusCode::CONFLICT, "{again}");
    db.cleanup().await
}

#[tokio::test]
async fn undo_after_the_resource_changed_is_409_and_leaves_it() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let steers = Arc::new(FakeSteers::default());
    let (convo, steer) = answered_with(&db, pause_answer(), steers.clone()).await?;
    let id = steer["id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("no steer id in {steer}"))?;
    let (status, stale) = undo(&convo, &bearer(TOKEN)?, id, 7).await?;
    assert_eq!(status, StatusCode::CONFLICT, "{stale}");
    let changed = json!({"paused": true, "by": "operator", "until": "tomorrow"});
    steers.touch(REMEDY, changed.clone());
    let (status, conflict) = undo(&convo, &bearer(TOKEN)?, id, 1).await?;
    assert_eq!(status, StatusCode::CONFLICT, "{conflict}");
    assert_eq!(conflict["error"], "conflict");
    assert_eq!(steers.state(REMEDY), changed);
    assert_eq!(last_steer(&db).await?["can_undo"], true);
    db.cleanup().await
}

#[tokio::test]
async fn out_of_set_proposal_is_refused_and_shown_as_refused() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let steers = Arc::new(FakeSteers::default());
    let answer = json!({"body": "I will restart the proxy.", "steer": {"kind": "restart_proxy", "now": true}});
    let (convo, steer) = answered_with(&db, answer, steers.clone()).await?;
    assert_eq!(steer["can_undo"], false, "{steer}");
    assert_eq!(steer["applied_at"], Value::Null);
    assert!(
        steer["refused"]
            .as_str()
            .is_some_and(|r| r.contains("restart_proxy")),
        "{steer}"
    );
    let range = json!({"body": "Raising the limit.", "steer": {"kind": "set_engineer_limit", "field": "tokens", "value": 999_999_999}});
    db.ask("range", "give it more tokens").await?;
    let convo = db.convo(FakeEngine::answering(vec![Ok(range)]), convo.steers.clone());
    tick(&convo).await?;
    let refused = last_steer(&db).await?;
    assert!(
        refused["refused"]
            .as_str()
            .is_some_and(|r| r.contains("between")),
        "{refused}"
    );
    assert_eq!(steers.state(REMEDY), json!({"paused": false}));
    db.cleanup().await
}

async fn seed_check(db: &Db) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO wright.checks (name, source, cadence, expects, probe, query, params, enabled)
         VALUES ('conversation_probe', 'capture_db', 'regular', 'nothing to find', 'sql', 'SELECT 1 WHERE false', '{}', false)
         ON CONFLICT (name) DO NOTHING",
    )
    .execute(db.handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn open_problem_steer_opens_an_operator_problem_and_undoes_it() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    seed_check(&db).await?;
    let steers = Steers {
        persist: db.handle.clone(),
    };
    let steers: Arc<dyn super::SteerApply> = Arc::new(steers);
    let answer = json!({"body": "Opened it.", "steer": {"kind": "open_problem", "check": "conversation_probe", "resource": "forks"}});
    db.ask("open", "look at why forks start cold").await?;
    let convo = db.convo(FakeEngine::answering(vec![Ok(answer)]), steers);
    tick(&convo).await?;
    let steer = last_steer(&db).await?;
    assert_eq!(steer["can_undo"], true, "{steer}");
    let open: Option<String> = sqlx::query_scalar(
        "SELECT state FROM wright.problems WHERE check_name = 'conversation_probe' AND scope = 'operator' AND resource = 'forks'",
    )
    .fetch_optional(db.handle.pool())
    .await?;
    assert_eq!(open.as_deref(), Some("open"));
    let id = steer["id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("no steer id in {steer}"))?;
    let (status, undone) = undo(&convo, &bearer(TOKEN)?, id, 1).await?;
    assert_eq!(status, StatusCode::OK, "{undone}");
    let gone: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wright.problems WHERE check_name = 'conversation_probe'",
    )
    .fetch_one(db.handle.pool())
    .await?;
    assert_eq!(gone, 0);
    assert!(
        db.handle
            .web()
            .wright_steer(id)
            .await?
            .is_some_and(|row| row.undone_at.is_some())
    );
    db.cleanup().await
}
