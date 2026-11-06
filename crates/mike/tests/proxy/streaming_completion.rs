use crate::anthropic_fixtures;
use crate::common;
use crate::sse;

use anthropic_fixtures::RequestBody;
use serde_json::{Value, json};
use sse::sse_body;
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn many_delta_events(chunks: usize) -> anyhow::Result<(Vec<(&'static str, String)>, String)> {
    let mut events: Vec<(&'static str, String)> = Vec::with_capacity(chunks + 5);
    let mut expected = String::new();

    events.push((
        "message_start",
        serde_json::to_string(&json!({
            "type": "message_start",
            "message": {
                "id": "msg_stuck_repro",
                "type": "message",
                "role": "assistant",
                "content": [],
                "model": "claude-opus-4-20250514",
                "stop_reason": null,
                "usage": {"input_tokens": 42, "output_tokens": 0}
            }
        }))?,
    ));
    events.push((
        "content_block_start",
        serde_json::to_string(&json!({
            "type": "content_block_start",
            "index": 0,
            "content_block": {"type": "text", "text": ""}
        }))?,
    ));
    for i in 0..chunks {
        let piece = format!("chunk-{i:05} ");
        expected.push_str(&piece);
        events.push((
            "content_block_delta",
            serde_json::to_string(&json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": piece}
            }))?,
        ));
    }
    events.push((
        "content_block_stop",
        serde_json::to_string(&json!({"type": "content_block_stop", "index": 0}))?,
    ));
    events.push((
        "message_delta",
        serde_json::to_string(&json!({
            "type": "message_delta",
            "delta": {"stop_reason": "end_turn"},
            "usage": {"output_tokens": 128}
        }))?,
    ));
    events.push((
        "message_stop",
        serde_json::to_string(&json!({"type": "message_stop"}))?,
    ));

    Ok((events, expected))
}

async fn wait_for_complete(
    client: &reqwest::Client,
    inspect_url: &str,
    id: u64,
    timeout: Duration,
) -> anyhow::Result<Value> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if std::time::Instant::now() > deadline {
            let detail: Value = client
                .get(format!("{inspect_url}/api/requests/{id}"))
                .send()
                .await?
                .json()
                .await?;
            anyhow::bail!("timed out waiting for request {id}; last state: {detail}");
        }
        let detail: Value = client
            .get(format!("{inspect_url}/api/requests/{id}"))
            .send()
            .await?
            .json()
            .await?;
        if detail["status"] == "Complete" {
            return Ok(detail);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[tokio::test]
async fn streaming_with_many_chunks_reaches_complete() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    let (events, _expected_text) = many_delta_events(400)?;
    let event_refs: Vec<(&str, &str)> = events.iter().map(|(e, d)| (*e, d.as_str())).collect();
    let raw_sse = sse_body(&event_refs);

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(raw_sse.as_bytes(), "text/event-stream")
                .insert_header("anthropic-ratelimit-requests-limit", "1000"),
        )
        .mount(&mock_server)
        .await;

    let (proxy_url, inspect_url) = common::start_proxy(&mock_server.uri()).await?;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header(
            "x-claude-code-session-id",
            "e0eebc99-9c0b-4ef8-bb6d-6bb9bd380e11",
        )
        .json(
            &RequestBody::default()
                .model("claude-opus-4-20250514")
                .max_tokens(4096)
                .stream(true)
                .content("stream me")
                .build(),
        )
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let body_bytes = resp.bytes().await?;
    assert!(!body_bytes.is_empty(), "client body must not be empty");

    let list: Value = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?
        .json()
        .await?;
    let id = list
        .as_array()
        .and_then(|a| a.first())
        .and_then(|r| r["id"].as_u64())
        .ok_or_else(|| anyhow::anyhow!("no request recorded"))?;

    let detail = wait_for_complete(&client, &inspect_url, id, Duration::from_secs(5)).await?;

    assert_eq!(detail["status"], "Complete");

    assert!(
        detail.get("accumulated_text").is_none(),
        "accumulated_text must not appear on the detail response: {detail}"
    );

    assert_eq!(detail["stop_reason"].as_str(), Some("end_turn"));
    assert!(detail["completed_at"].is_string());

    assert_eq!(detail["token_usage"]["input_tokens"], 42);
    assert_eq!(detail["token_usage"]["output_tokens"], 128);

    let headers = detail["response_headers"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("response_headers must be an array, got: {detail}"))?;
    assert!(
        headers.iter().any(|pair| pair
            .as_array()
            .and_then(|p| p.first())
            .and_then(|k| k.as_str())
            == Some("anthropic-ratelimit-requests-limit")),
        "streaming detail must include upstream ratelimit headers"
    );

    Ok(())
}
