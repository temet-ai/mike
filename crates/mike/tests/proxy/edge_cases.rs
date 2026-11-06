use crate::anthropic_fixtures;
use crate::common;
use crate::db_helpers;
use crate::raw_sse_mock;

use anthropic_fixtures::RequestBody;
use serde_json::json;

use raw_sse_mock::MockSseEvent;

#[tokio::test]
async fn test_mid_stream_error_forwarded() -> anyhow::Result<()> {
    let events = vec![
        MockSseEvent::new(
            "message_start",
            &serde_json::to_string(&json!({
                "type": "message_start",
                "message": {
                    "id": "msg_error_test",
                    "type": "message",
                    "role": "assistant",
                    "content": [],
                    "model": "claude-sonnet-4-20250514",
                    "stop_reason": null,
                    "usage": {"input_tokens": 10, "output_tokens": 0}
                }
            }))?,
        ),
        MockSseEvent::new(
            "content_block_start",
            &serde_json::to_string(&json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": {"type": "text", "text": ""}
            }))?,
        ),
        MockSseEvent::new(
            "content_block_delta",
            &serde_json::to_string(&json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": "Hello before error"}
            }))?,
        ),
        MockSseEvent::new(
            "error",
            &serde_json::to_string(&json!({
                "type": "error",
                "error": {
                    "type": "overloaded_error",
                    "message": "Overloaded"
                }
            }))?,
        ),
    ];

    let addr = raw_sse_mock::raw_sse_mock(events, None).await?;
    let (proxy_url, inspect_url) = common::start_proxy(&format!("http://{addr}")).await?;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&RequestBody::default().stream(true).build())
        .send()
        .await?;

    assert_eq!(resp.status(), 200);

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.starts_with("text/event-stream"),
        "content-type must be text/event-stream, got: {content_type}"
    );

    let body_bytes = resp.bytes().await?;
    let body_str = String::from_utf8_lossy(&body_bytes);

    let received_events: Vec<&str> = body_str
        .lines()
        .filter_map(|line| line.strip_prefix("event: "))
        .collect();

    assert_eq!(
        received_events,
        vec![
            "message_start",
            "content_block_start",
            "content_block_delta",
            "error"
        ],
        "all events including the error must be forwarded"
    );

    assert!(
        body_str.contains("overloaded_error"),
        "error event data must be forwarded: {body_str}"
    );

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let health_resp = client
        .get(format!("{inspect_url}/api/health"))
        .send()
        .await?;
    assert_eq!(
        health_resp.status(),
        200,
        "proxy must remain healthy after mid-stream error"
    );

    Ok(())
}

#[tokio::test]
async fn test_upstream_drop_mid_stream() -> anyhow::Result<()> {
    let events = vec![
        MockSseEvent::new(
            "message_start",
            &serde_json::to_string(&json!({
                "type": "message_start",
                "message": {
                    "id": "msg_drop_test",
                    "type": "message",
                    "role": "assistant",
                    "content": [],
                    "model": "claude-sonnet-4-20250514",
                    "stop_reason": null,
                    "usage": {"input_tokens": 5, "output_tokens": 0}
                }
            }))?,
        ),
        MockSseEvent::new(
            "content_block_delta",
            &serde_json::to_string(&json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": "before drop"}
            }))?,
        ),
    ];

    let addr = raw_sse_mock::raw_sse_mock(events, Some(2)).await?;
    let (proxy_url, inspect_url) = common::start_proxy(&format!("http://{addr}")).await?;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&RequestBody::default().stream(true).build())
        .send()
        .await?;

    assert_eq!(resp.status(), 200);

    let body_result = tokio::time::timeout(std::time::Duration::from_secs(5), resp.bytes()).await;

    assert!(
        body_result.is_ok(),
        "stream must terminate within timeout (not hang)"
    );

    let body_bytes = body_result?;
    if let Ok(bytes) = &body_bytes {
        let body_str = String::from_utf8_lossy(bytes);
        if body_str.contains("event:") {
            assert!(
                body_str.contains("message_start"),
                "should contain events sent before drop"
            );
        }
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let health_resp = client
        .get(format!("{inspect_url}/api/health"))
        .send()
        .await?;
    assert_eq!(
        health_resp.status(),
        200,
        "proxy must remain healthy after upstream drop"
    );

    Ok(())
}

#[tokio::test]
async fn upstream_drop_mid_stream_db_row_status_is_error() -> anyhow::Result<()> {
    let Some(proxy) = db_helpers::start_proxy_with_db("http://127.0.0.1:1").await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping upstream_drop_mid_stream_db_row_status_is_error"
        );
        return Ok(());
    };

    let events = vec![
        MockSseEvent::new(
            "message_start",
            &serde_json::to_string(&json!({
                "type": "message_start",
                "message": {
                    "id": "msg_db_drop_test",
                    "type": "message",
                    "role": "assistant",
                    "content": [],
                    "model": "claude-sonnet-4-20250514",
                    "stop_reason": null,
                    "usage": {"input_tokens": 5, "output_tokens": 0}
                }
            }))?,
        ),
        MockSseEvent::new(
            "content_block_delta",
            &serde_json::to_string(&json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": "before drop"}
            }))?,
        ),
    ];
    let upstream_addr = raw_sse_mock::raw_sse_mock(events, Some(2)).await?;

    let (proxy_url, _inspect_url, _shutdown) = db_helpers::start_proxy_with_db_at(
        &format!("http://{upstream_addr}"),
        &proxy.test_db.database_url,
    )
    .await?;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header(
            "x-claude-code-session-id",
            "d0eebc99-9c0b-4ef8-bb6d-6bb9bd380d11",
        )
        .json(&RequestBody::default().stream(true).build())
        .send()
        .await?;

    assert_eq!(resp.status(), 200);

    let body_result = tokio::time::timeout(std::time::Duration::from_secs(5), resp.bytes()).await;
    assert!(body_result.is_ok(), "stream must terminate within timeout");

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    let status = loop {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT s.code FROM requests.requests r
             JOIN requests.statuses s ON s.id = r.status_id
             ORDER BY r.id DESC LIMIT 1",
        )
        .fetch_optional(&proxy.pool)
        .await?;
        let status = row.map(|(s,)| s).unwrap_or_else(|| "no row".to_string());
        let settled = status != "pending" && status != "no row";
        if settled || tokio::time::Instant::now() >= deadline {
            break status;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    };

    assert_eq!(
        status, "error",
        "DB row status must be 'error' after upstream drop, got: {status:?}"
    );

    proxy.test_db.cleanup().await?;
    Ok(())
}
