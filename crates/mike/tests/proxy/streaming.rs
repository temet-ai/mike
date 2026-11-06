use crate::anthropic_fixtures;
use crate::common;
use crate::sse;

use anthropic_fixtures::{MockResponse, RequestBody};
use serde_json::json;
use sse::sse_body;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn claude_sse_events() -> anyhow::Result<Vec<(&'static str, String)>> {
    Ok(vec![
        (
            "message_start",
            serde_json::to_string(&json!({
                "type": "message_start",
                "message": {
                    "id": "msg_test123",
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
        ("ping", "{}".to_string()),
        (
            "content_block_delta",
            serde_json::to_string(&json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": "Hello!"}
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
            serde_json::to_string(&json!({
                "type": "message_stop"
            }))?,
        ),
    ])
}

#[tokio::test]
async fn test_sse_streaming_passthrough() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    let events = claude_sse_events()?;
    let event_refs: Vec<(&str, &str)> = events.iter().map(|(e, d)| (*e, d.as_str())).collect();
    let raw_sse = sse_body(&event_refs);

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(raw_sse.as_bytes(), "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let (proxy_url, _inspect_url) = common::start_proxy(&mock_server.uri()).await?;
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

    assert_eq!(
        body_str.as_ref(),
        raw_sse.as_str(),
        "SSE body must be byte-identical to upstream"
    );

    let expected_events = [
        "message_start",
        "content_block_start",
        "ping",
        "content_block_delta",
        "content_block_stop",
        "message_delta",
        "message_stop",
    ];

    let received_events: Vec<&str> = body_str
        .lines()
        .filter_map(|line| line.strip_prefix("event: "))
        .collect();

    assert_eq!(
        received_events, expected_events,
        "all SSE event types must arrive in order"
    );

    Ok(())
}

#[tokio::test]
async fn test_client_disconnect_mid_stream() -> anyhow::Result<()> {
    use futures::StreamExt;
    use tokio::time::{Duration, sleep};

    let mock_server = MockServer::start().await;

    let mut events: Vec<(&str, String)> = Vec::new();
    events.push((
        "message_start",
        serde_json::to_string(&json!({
            "type": "message_start",
            "message": {
                "id": "msg_disconnect_test",
                "type": "message",
                "role": "assistant",
                "content": [],
                "model": "claude-sonnet-4-20250514",
                "stop_reason": null,
                "usage": {"input_tokens": 10, "output_tokens": 0}
            }
        }))?,
    ));
    for i in 0..50 {
        events.push((
            "content_block_delta",
            serde_json::to_string(&json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": format!("chunk-{i} ")}
            }))?,
        ));
    }

    let event_refs: Vec<(&str, &str)> = events.iter().map(|(e, d)| (*e, d.as_str())).collect();
    let raw_sse = sse_body(&event_refs);

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(raw_sse.as_bytes(), "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let (proxy_url, inspect_url) = common::start_proxy(&mock_server.uri()).await?;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("content-type", "application/json")
        .json(&RequestBody::default().stream(true).build())
        .send()
        .await?;

    assert_eq!(resp.status(), 200);

    let mut stream = resp.bytes_stream();
    let first_chunk = stream.next().await;
    assert!(
        first_chunk.is_some(),
        "should receive at least one chunk before disconnect"
    );
    drop(stream);

    sleep(Duration::from_millis(100)).await;

    let health_resp = client
        .get(format!("{inspect_url}/api/health"))
        .send()
        .await?;
    assert_eq!(
        health_resp.status(),
        200,
        "proxy must remain healthy after client disconnect"
    );

    Ok(())
}

#[tokio::test]
async fn test_non_streaming_still_works_after_sse_branch() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    let response_body = MockResponse::new("msg_nonstream")
        .text("Hello from non-stream!")
        .usage(10, 8)
        .build();

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(&response_body)
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let (proxy_url, _inspect_url) = common::start_proxy(&mock_server.uri()).await?;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("content-type", "application/json")
        .json(&RequestBody::default().build())
        .send()
        .await?;

    assert_eq!(resp.status(), 200);

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.starts_with("application/json"),
        "non-streaming response should have application/json, got: {content_type}"
    );

    let body: serde_json::Value = resp.json().await?;
    assert_eq!(body, response_body, "non-streaming body must match mock");

    Ok(())
}
