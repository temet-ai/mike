mod backfill;
mod compaction;
mod round_trip;

use anyhow::Context;
use serde_json::Value as JsonValue;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::request_write::IdMode;

pub(super) struct Capture<'a> {
    pub session_id: &'a str,
    pub original: &'a JsonValue,
    pub sent: Option<&'a JsonValue>,
}

pub(super) async fn capture(handle: &PersistHandle, capture: Capture<'_>) -> anyhow::Result<i64> {
    let mut req = crate::store::tests::make_test_request(None, capture.session_id);
    req.request_body = Some(bytes::Bytes::from(capture.original.to_string()));
    req.sent_body = capture
        .sent
        .map(|sent| bytes::Bytes::from(sent.to_string()));
    let outcome = handle
        .insert_request_initial(&req, true, IdMode::Mint)
        .await?;
    Ok(outcome.id)
}

pub(super) async fn clear_request_body(
    handle: &PersistHandle,
    request_id: i64,
) -> anyhow::Result<()> {
    sqlx::query("UPDATE requests.requests SET request_body = NULL WHERE id = $1")
        .bind(request_id)
        .execute(&handle.pool)
        .await?;
    Ok(())
}

pub(super) async fn sent_of(handle: &PersistHandle, request_id: i64) -> anyhow::Result<JsonValue> {
    let body = handle
        .fetch_request_sent_body_by_id(request_id as u64)
        .await?
        .context("the sent body must be served")?;
    Ok(serde_json::from_slice(&body)?)
}

pub(super) async fn original_of(
    handle: &PersistHandle,
    request_id: i64,
) -> anyhow::Result<JsonValue> {
    let body = handle
        .fetch_request_body_by_id(request_id as u64)
        .await?
        .context("the original body must be served")?;
    Ok(serde_json::from_slice(&body.bytes)?)
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct SentColumns {
    pub has_envelope: bool,
    pub count: Option<i32>,
    pub ords: Vec<i32>,
}

pub(super) async fn sent_columns(
    handle: &PersistHandle,
    request_id: i64,
) -> anyhow::Result<SentColumns> {
    let (has_envelope, count): (bool, Option<i32>) = sqlx::query_as(
        "SELECT sent_envelope IS NOT NULL, sent_message_count FROM requests.requests WHERE id = $1",
    )
    .bind(request_id)
    .fetch_one(&handle.pool)
    .await?;
    let ords: Vec<i32> = sqlx::query_scalar(
        "SELECT ord FROM requests.sent_messages WHERE request_id = $1 ORDER BY ord",
    )
    .bind(request_id)
    .fetch_all(&handle.pool)
    .await?;
    Ok(SentColumns {
        has_envelope,
        count,
        ords,
    })
}
