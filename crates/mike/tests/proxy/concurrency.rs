use crate::anthropic_fixtures;
use crate::common;

use anthropic_fixtures::RequestBody;
use futures::future::join_all;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn sse_body() -> anyhow::Result<String> {
    let events: Vec<(&str, String)> = vec![
        (
            "message_start",
            serde_json::to_string(&json!({
                "type": "message_start",
                "message": {
                    "id": "msg_concurrent",
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
            serde_json::to_string(&json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": {"type": "text", "text": ""}
            }))?,
        ),
        (
            "content_block_delta",
            serde_json::to_string(&json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": "concurrent-response"}
            }))?,
        ),
        (
            "content_block_stop",
            serde_json::to_string(&json!({
                "type": "content_block_stop",
                "index": 0
            }))?,
        ),
        (
            "message_delta",
            serde_json::to_string(&json!({
                "type": "message_delta",
                "delta": {"stop_reason": "end_turn"},
                "usage": {"output_tokens": 5}
            }))?,
        ),
        (
            "message_stop",
            serde_json::to_string(&json!({"type": "message_stop"}))?,
        ),
    ];

    let mut body = String::new();
    for (event_type, data) in &events {
        body.push_str(&format!("event: {event_type}\ndata: {data}\n\n"));
    }
    Ok(body)
}

#[tokio::test]
async fn test_concurrent_streams_isolated() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    let canonical_sse = sse_body()?;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(canonical_sse.as_bytes(), "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let (proxy_url, _inspect_url) = common::start_proxy(&mock_server.uri()).await?;
    let client = reqwest::Client::new();

    let num_concurrent = 20;

    let futures: Vec<_> = (0..num_concurrent)
        .map(|i| {
            let client = client.clone();
            let url = format!("{proxy_url}/v1/messages");
            let expected_len = canonical_sse.len();

            tokio::spawn(async move {
                let resp = client
                    .post(&url)
                    .header("x-api-key", format!("sk-test-{i}"))
                    .header("anthropic-version", "2023-06-01")
                    .header("content-type", "application/json")
                    .json(
                        &RequestBody::default()
                            .content(format!("Hello from {i}"))
                            .stream(true)
                            .build(),
                    )
                    .send()
                    .await
                    .map_err(|e| anyhow::anyhow!("request {i} failed: {e}"))?;

                let status = resp.status().as_u16();
                let content_type = resp
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();
                let body_bytes = resp
                    .bytes()
                    .await
                    .map_err(|e| anyhow::anyhow!("body read {i} failed: {e}"))?;
                let body_str = String::from_utf8_lossy(&body_bytes).to_string();

                Ok::<(usize, u16, String, String, usize), anyhow::Error>((
                    i,
                    status,
                    content_type,
                    body_str,
                    expected_len,
                ))
            })
        })
        .collect();

    let results = join_all(futures).await;

    for result in &results {
        let join_result = result
            .as_ref()
            .map_err(|e| anyhow::anyhow!("join error: {e}"))?;
        let (i, status, content_type, body_str, expected_len) =
            join_result.as_ref().map_err(|e| anyhow::anyhow!("{e}"))?;

        assert_eq!(*status, 200, "request {i}: expected 200, got {status}");
        assert!(
            content_type.starts_with("text/event-stream"),
            "request {i}: expected text/event-stream, got {content_type}"
        );

        let received_events: Vec<&str> = body_str
            .lines()
            .filter_map(|line| line.strip_prefix("event: "))
            .collect();
        let expected_events = [
            "message_start",
            "content_block_start",
            "content_block_delta",
            "content_block_stop",
            "message_delta",
            "message_stop",
        ];
        assert_eq!(
            received_events, expected_events,
            "request {i}: SSE events mismatch"
        );

        assert_eq!(
            body_str.len(),
            *expected_len,
            "request {i}: body length mismatch — possible cross-contamination"
        );
    }

    let received = mock_server.received_requests().await;
    let received = received
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    assert_eq!(
        received.len(),
        num_concurrent,
        "upstream should receive exactly {num_concurrent} requests"
    );

    Ok(())
}
