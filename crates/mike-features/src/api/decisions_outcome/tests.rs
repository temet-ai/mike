use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{OutcomeRequest, post_outcome};
use crate::config::ProxyConfig;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;

async fn seed_record(handle: &PersistHandle) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO decisions.records
             (point, mode, question_set, question_set_hash, model_requested, state, questions,
              request_body, outcome)
         VALUES ('prod_detect', 'on', 'test_v1', 'h', 'typesafe/jev-1.14', '{}'::jsonb,
                 '{}'::jsonb, '{}'::jsonb, 'ok')
         RETURNING id",
    )
    .fetch_one(handle.pool())
    .await?)
}

fn request(label: &str) -> Json<OutcomeRequest> {
    Json(OutcomeRequest {
        label: label.to_owned(),
        reason: "the nudge matched the message".to_owned(),
        evidence: Some(json!({"request_id": 7})),
    })
}

async fn status_of(
    state: &AppState,
    id: i64,
    headers: &HeaderMap,
    label: &str,
) -> Result<i64, StatusCode> {
    post_outcome(
        State(state.clone()),
        Path(id),
        headers.clone(),
        request(label),
    )
    .await
    .map(|Json(w)| w.outcome_id)
    .map_err(|(s, _)| s)
}

#[tokio::test]
async fn post_outcome_writes_one_review_and_refuses_the_rest() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let mut state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    state.persist = Some(handle.clone());
    let record = seed_record(&handle).await?;
    let reviewer = Uuid::new_v4().to_string();
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-claude-code-session-id",
        HeaderValue::from_str(&reviewer)?,
    );

    assert_eq!(
        status_of(&state, record, &headers, "maybe").await,
        Err(StatusCode::BAD_REQUEST)
    );
    let outcome = status_of(&state, record, &headers, "right")
        .await
        .map_err(|s| anyhow::anyhow!("first label refused with {s}"))?;
    assert_eq!(
        status_of(&state, record, &headers, "wrong").await,
        Err(StatusCode::CONFLICT)
    );
    assert_eq!(
        status_of(&state, i64::MAX, &headers, "right").await,
        Err(StatusCode::NOT_FOUND)
    );

    let (source, evidence): (String, Value) =
        sqlx::query_as("SELECT label_source, evidence FROM decisions.outcomes WHERE id = $1")
            .bind(outcome)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(source, "claude_review");
    assert_eq!(
        evidence,
        json!({"request_id": 7, "reason": "the nudge matched the message", "reviewer": reviewer})
    );

    handle.pool().close().await;
    state.shutdown().cancel();
    db.cleanup().await
}
