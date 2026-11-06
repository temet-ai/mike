use crate::anthropic_fixtures;
use crate::common;

use anthropic_fixtures::RequestBody;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::time::timeout;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn ws_url(inspect_url: &str) -> String {
    let authority = inspect_url.trim_start_matches("http://");
    format!("ws://{authority}/api/ws")
}

async fn read_until<S, F>(ws: &mut S, mut predicate: F) -> anyhow::Result<Value>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
    F: FnMut(&Value) -> bool,
{
    loop {
        let msg = timeout(Duration::from_secs(5), ws.next())
            .await?
            .ok_or_else(|| anyhow::anyhow!("ws stream closed before match"))??;
        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Close(_) => anyhow::bail!("ws closed before match"),
            _ => continue,
        };
        let value: Value = serde_json::from_str(&text)?;
        if predicate(&value) {
            return Ok(value);
        }
    }
}

fn claude_sse_body() -> anyhow::Result<String> {
    let events = [
        (
            "message_start",
            serde_json::to_string(&json!({
                "type": "message_start",
                "message": {
                    "id": "msg_ws_test",
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
                "delta": {"type": "text_delta", "text": "hello"}
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

async fn drive_sse_request(proxy_url: &str) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header(
            "x-claude-code-session-id",
            "f0eebc99-9c0b-4ef8-bb6d-6bb9bd380f11",
        )
        .json(&RequestBody::default().stream(true).build())
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let _ = resp.bytes().await?;
    Ok(())
}

#[tokio::test]
async fn ws_delivers_events_from_real_sse_flow() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    let sse_body = claude_sse_body()?;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(sse_body.as_bytes(), "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let (proxy_url, inspect_url) = common::start_proxy(&mock_server.uri()).await?;

    let (mut ws, _resp) = connect_async(ws_url(&inspect_url)).await?;

    let proxy_url_clone = proxy_url.clone();
    let request_task = tokio::spawn(async move { drive_sse_request(&proxy_url_clone).await });

    let model_confirmed = read_until(&mut ws, |v| {
        v.get("type").and_then(Value::as_str) == Some("ModelConfirmed")
    })
    .await?;
    assert_eq!(
        model_confirmed["model"].as_str(),
        Some("claude-sonnet-4-20250514"),
    );
    assert!(model_confirmed["id"].is_u64(), "id must be numeric");

    let text_delta = read_until(&mut ws, |v| {
        v.get("type").and_then(Value::as_str) == Some("TextDelta")
    })
    .await?;
    assert_eq!(text_delta["text"].as_str(), Some("hello"));

    let complete = read_until(&mut ws, |v| {
        v.get("type").and_then(Value::as_str) == Some("ResponseComplete")
    })
    .await?;
    assert_eq!(complete["stop_reason"].as_str(), Some("end_turn"));

    ws.send(Message::Close(None)).await.ok();
    request_task.await??;
    Ok(())
}

#[tokio::test]
async fn ws_streaming_started_precedes_deltas_with_expected_shape() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    let sse_body = claude_sse_body()?;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(sse_body.as_bytes(), "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let (proxy_url, inspect_url) = common::start_proxy(&mock_server.uri()).await?;

    let (mut ws, _resp) = connect_async(ws_url(&inspect_url)).await?;

    let proxy_url_clone = proxy_url.clone();
    let request_task = tokio::spawn(async move { drive_sse_request(&proxy_url_clone).await });

    let started = read_until(&mut ws, |v| {
        v.get("type").and_then(Value::as_str) == Some("StreamingStarted")
    })
    .await?;
    assert!(
        started["id"].is_u64(),
        "id must be numeric, got {started:?}"
    );
    assert_eq!(
        started["status"].as_u64(),
        Some(200),
        "status must echo the upstream HTTP status"
    );
    assert!(
        started["ttfb_ms"].is_u64(),
        "ttfb_ms must be numeric, got {started:?}"
    );
    assert_eq!(
        started["model"].as_str(),
        Some("claude-sonnet-4-20250514"),
        "model must carry the requested model name"
    );

    let text_delta = read_until(&mut ws, |v| {
        v.get("type").and_then(Value::as_str) == Some("TextDelta")
    })
    .await?;
    assert_eq!(text_delta["text"].as_str(), Some("hello"));

    ws.send(Message::Close(None)).await.ok();
    request_task.await??;
    Ok(())
}

#[tokio::test]
async fn ws_client_close_does_not_break_proxy() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    let sse_body = claude_sse_body()?;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(sse_body.as_bytes(), "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let (proxy_url, inspect_url) = common::start_proxy(&mock_server.uri()).await?;

    let (mut ws, _resp) = connect_async(ws_url(&inspect_url)).await?;
    ws.send(Message::Close(None)).await?;
    drop(ws);

    tokio::time::sleep(Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let health = client
        .get(format!("{inspect_url}/api/health"))
        .send()
        .await?;
    assert_eq!(health.status(), 200);

    drive_sse_request(&proxy_url).await?;
    Ok(())
}

#[tokio::test]
async fn ws_lagged_client_receives_lagged_frame() -> anyhow::Result<()> {
    let mut events: Vec<(String, String)> = Vec::new();
    events.push((
        "message_start".to_string(),
        serde_json::to_string(&json!({
            "type": "message_start",
            "message": {
                "id": "msg_lag_test",
                "type": "message",
                "role": "assistant",
                "content": [],
                "model": "claude-sonnet-4-20250514",
                "stop_reason": null,
                "usage": {"input_tokens": 1, "output_tokens": 0}
            }
        }))?,
    ));
    events.push((
        "content_block_start".to_string(),
        serde_json::to_string(&json!({
            "type": "content_block_start",
            "index": 0,
            "content_block": {"type": "text", "text": ""}
        }))?,
    ));
    for i in 0..400 {
        events.push((
            "content_block_delta".to_string(),
            serde_json::to_string(&json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": format!("t{i}")}
            }))?,
        ));
    }
    events.push((
        "content_block_stop".to_string(),
        serde_json::to_string(&json!({"type": "content_block_stop", "index": 0}))?,
    ));
    events.push((
        "message_delta".to_string(),
        serde_json::to_string(&json!({
            "type": "message_delta",
            "delta": {"stop_reason": "end_turn"},
            "usage": {"output_tokens": 400}
        }))?,
    ));
    events.push((
        "message_stop".to_string(),
        serde_json::to_string(&json!({"type": "message_stop"}))?,
    ));

    let mut sse_body = String::new();
    for (event_type, data) in &events {
        sse_body.push_str(&format!("event: {event_type}\ndata: {data}\n\n"));
    }

    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(sse_body.as_bytes(), "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let (proxy_url, inspect_url) = common::start_proxy(&mock_server.uri()).await?;

    let (mut ws, _resp) = connect_async(ws_url(&inspect_url)).await?;

    drive_sse_request(&proxy_url).await?;

    tokio::time::sleep(Duration::from_millis(200)).await;

    let lagged = read_until(&mut ws, |v| {
        v.get("type").and_then(Value::as_str) == Some("Lagged")
    })
    .await?;
    assert!(
        lagged["skipped"].as_u64().is_some_and(|n| n > 0),
        "Lagged frame must report a positive skipped count, got {lagged:?}"
    );

    ws.send(Message::Close(None)).await.ok();
    Ok(())
}
