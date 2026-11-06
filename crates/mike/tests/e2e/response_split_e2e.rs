use crate::anthropic_fixtures;
use crate::db_helpers;
use crate::sse;

use anthropic_fixtures::{MockResponse, RequestBody};
use db_helpers::start_proxy_with_db;
use serde_json::json;
use sqlx::Row;
use sse::sse_body;
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn streaming_events() -> anyhow::Result<String> {
    Ok(sse_body(&[
        (
            "message_start",
            &serde_json::to_string(&json!({
                "type": "message_start",
                "message": {
                    "id": "msg_rr_e2e_streaming",
                    "type": "message",
                    "role": "assistant",
                    "content": [],
                    "model": "claude-sonnet-4-20250514",
                    "stop_reason": null,
                    "usage": {"input_tokens": 10, "output_tokens": 0}
                }
            }))?,
        ),
        (
            "content_block_start",
            &serde_json::to_string(&json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": {"type": "text", "text": ""}
            }))?,
        ),
        (
            "content_block_delta",
            &serde_json::to_string(&json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": "Hi"}
            }))?,
        ),
        (
            "content_block_stop",
            &serde_json::to_string(&json!({"type": "content_block_stop", "index": 0}))?,
        ),
        (
            "message_delta",
            &serde_json::to_string(&json!({
                "type": "message_delta",
                "delta": {"stop_reason": "end_turn", "stop_sequence": null},
                "usage": {"output_tokens": 1}
            }))?,
        ),
        (
            "message_stop",
            &serde_json::to_string(&json!({"type": "message_stop"}))?,
        ),
    ]))
}

fn non_streaming_mock_body() -> serde_json::Value {
    MockResponse::new("msg_rr_e2e_non_streaming")
        .text("Hello")
        .usage(15, 8)
        .build()
}

async fn wait_for_response_row(
    pool: &sqlx::PgPool,
) -> anyhow::Result<(i16, Option<String>, serde_json::Value, usize)> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let row = sqlx::query(
            "SELECT rr.response_status, sr.code AS stop_reason, rr.token_usage,
                    octet_length(rr.response_body) AS body_len
             FROM responses.responses rr
             LEFT JOIN responses.stop_reasons sr ON sr.id = rr.stop_reason_id
             ORDER BY rr.completed_at DESC LIMIT 1",
        )
        .fetch_optional(pool)
        .await?;

        if let Some(r) = row {
            let status: i16 = r.get("response_status");
            let stop_reason: Option<String> = r.get("stop_reason");
            let token_usage: serde_json::Value = r.get("token_usage");
            let body_len: i32 = r.get("body_len");
            return Ok((status, stop_reason, token_usage, body_len as usize));
        }

        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!("timed out waiting for request_responses row");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn wait_for_response_with_status(
    pool: &sqlx::PgPool,
    expected_status: i16,
) -> anyhow::Result<(i16, Option<String>)> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let row: Option<(i16, Option<String>)> = sqlx::query_as(
            "SELECT rr.response_status, sr.code
             FROM responses.responses rr
             LEFT JOIN responses.stop_reasons sr ON sr.id = rr.stop_reason_id
             WHERE rr.response_status = $1
             ORDER BY rr.completed_at DESC LIMIT 1",
        )
        .bind(expected_status)
        .fetch_optional(pool)
        .await?;

        if let Some(r) = row {
            return Ok(r);
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!(
                "timed out waiting for request_responses row with status={}",
                expected_status
            );
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn e2e_non_streaming_response_in_request_responses() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(non_streaming_mock_body())
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping e2e_non_streaming test");
        return Ok(());
    };
    let (proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = uuid::Uuid::new_v4().to_string();
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", &session_id)
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .json(
            &RequestBody::default()
                .content("e2e non-streaming rr test")
                .max_tokens(16)
                .build(),
        )
        .send()
        .await?;
    anyhow::ensure!(
        resp.status() == 200,
        "expected 200 from proxy, got {}",
        resp.status()
    );
    let _ = resp.bytes().await?;

    let (status, _stop_reason, token_usage, body_len) = wait_for_response_row(&pool).await?;

    assert_eq!(status, 200i16, "response_status must be 200");
    assert!(
        token_usage.is_object(),
        "token_usage must be a JSON object (may be empty for non-streaming); got: {}",
        token_usage
    );
    assert!(
        body_len > 0,
        "non-streaming response_body must be non-empty (got {} bytes)",
        body_len
    );

    shutdown.cancel();
    tokio::time::sleep(Duration::from_millis(100)).await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn e2e_streaming_response_in_request_responses() -> anyhow::Result<()> {
    let raw_sse = streaming_events()?;

    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(raw_sse.as_bytes(), "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping e2e_streaming test");
        return Ok(());
    };
    let (proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = uuid::Uuid::new_v4().to_string();
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", &session_id)
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .json(
            &RequestBody::default()
                .content("e2e streaming rr test")
                .max_tokens(16)
                .stream(true)
                .build(),
        )
        .send()
        .await?;
    anyhow::ensure!(
        resp.status() == 200,
        "expected 200 from proxy, got {}",
        resp.status()
    );
    let _ = resp.bytes().await?;

    let (status, stop_reason, token_usage, body_len) = wait_for_response_row(&pool).await?;

    assert_eq!(status, 200i16, "response_status must be 200 for streaming");
    assert_eq!(
        stop_reason.as_deref(),
        Some("end_turn"),
        "stop_reason must be 'end_turn' from message_delta"
    );
    assert!(
        body_len > 0,
        "streaming response_body must be the assembled assistant message (got {body_len} bytes)"
    );
    let body_bytes: Vec<u8> = sqlx::query_scalar(
        "SELECT rr.response_body
         FROM responses.responses rr
         ORDER BY rr.completed_at DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await?;
    let parsed: serde_json::Value = serde_json::from_slice(&body_bytes)?;
    assert_eq!(
        parsed["type"].as_str(),
        Some("message"),
        "assembled streaming body must be an assistant message object: {parsed}"
    );
    assert_eq!(parsed["role"].as_str(), Some("assistant"));
    assert!(
        !token_usage.is_null(),
        "token_usage must not be null for a completed streaming response"
    );

    shutdown.cancel();
    tokio::time::sleep(Duration::from_millis(100)).await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn e2e_429_status_in_request_responses() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(429)
                .set_body_json(
                    json!({"error": {"type": "rate_limit_error", "message": "Too many requests"}}),
                )
                .insert_header("retry-after", "30")
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping e2e_429 test");
        return Ok(());
    };
    let (proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = uuid::Uuid::new_v4().to_string();
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", &session_id)
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .json(
            &RequestBody::default()
                .content("e2e 429 rr test")
                .max_tokens(16)
                .build(),
        )
        .send()
        .await?;
    anyhow::ensure!(
        resp.status() == 429,
        "expected 429 from proxy, got {}",
        resp.status()
    );
    let _ = resp.bytes().await?;

    let (status, _stop_reason) = wait_for_response_with_status(&pool, 429).await?;
    assert_eq!(
        status, 429i16,
        "request_responses.response_status must be 429"
    );

    shutdown.cancel();
    tokio::time::sleep(Duration::from_millis(100)).await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn e2e_request_responses_not_duplicated_on_retry() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(non_streaming_mock_body())
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping idempotent retry test");
        return Ok(());
    };
    let (proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = uuid::Uuid::new_v4().to_string();
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", &session_id)
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .json(
            &RequestBody::default()
                .content("e2e idempotent rr test")
                .max_tokens(16)
                .build(),
        )
        .send()
        .await?;
    anyhow::ensure!(resp.status() == 200, "expected 200, got {}", resp.status());
    let _ = resp.bytes().await?;

    let _ = wait_for_response_row(&pool).await?;

    let request_id: i64 = sqlx::query_scalar(
        "SELECT request_id FROM responses.responses
         ORDER BY completed_at DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await?;

    let affected = sqlx::query(
        "INSERT INTO responses.responses (
             request_id,
             response_status, response_headers, response_body,
             token_usage, stop_reason_id, completed_at
         )
         SELECT $1, 200, '[]'::jsonb, '\\x'::bytea, '{}'::jsonb, sr.id, NOW()
         FROM (SELECT 'end_turn'::text AS code) AS v
         LEFT JOIN responses.stop_reasons sr ON sr.code = v.code
         WHERE NOT EXISTS (
             SELECT 1 FROM responses.responses WHERE request_id = $1
         )",
    )
    .bind(request_id)
    .execute(&pool)
    .await?;
    assert_eq!(
        affected.rows_affected(),
        0,
        "duplicate insert must affect 0 rows (insert-if-not-exists)"
    );

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM responses.responses WHERE request_id = $1")
            .bind(request_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        count, 1,
        "exactly 1 request_responses row per request after idempotent retry"
    );

    shutdown.cancel();
    tokio::time::sleep(Duration::from_millis(100)).await;
    test_db.cleanup().await?;
    Ok(())
}
