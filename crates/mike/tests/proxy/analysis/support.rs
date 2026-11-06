use crate::anthropic_fixtures;

use anthropic_fixtures::RequestBody;
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub(super) fn sse_body_with_trigger(trigger: &str) -> String {
    let text_escaped = trigger.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        concat!(
            "event: message_start\n",
            "data: {{\"type\":\"message_start\",\"message\":{{\"id\":\"msg_analysis\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-20250514\",\"stop_reason\":null,\"usage\":{{\"input_tokens\":5,\"output_tokens\":0}}}}}}\n\n",
            "event: content_block_start\n",
            "data: {{\"type\":\"content_block_start\",\"index\":0,\"content_block\":{{\"type\":\"text\",\"text\":\"\"}}}}\n\n",
            "event: content_block_delta\n",
            "data: {{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{{\"type\":\"text_delta\",\"text\":\"{text}\"}}}}\n\n",
            "event: content_block_stop\n",
            "data: {{\"type\":\"content_block_stop\",\"index\":0}}\n\n",
            "event: message_delta\n",
            "data: {{\"type\":\"message_delta\",\"delta\":{{\"stop_reason\":\"end_turn\"}},\"usage\":{{\"output_tokens\":5}}}}\n\n",
            "event: message_stop\n",
            "data: {{\"type\":\"message_stop\"}}\n\n",
        ),
        text = text_escaped
    )
}

pub(super) async fn post_and_get_id(
    client: &reqwest::Client,
    proxy_url: &str,
    inspect_url: &str,
) -> anyhow::Result<u64> {
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("content-type", "application/json")
        .header(
            "x-claude-code-session-id",
            "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
        )
        .json(
            &RequestBody::default()
                .content("Hi")
                .max_tokens(64)
                .stream(true)
                .build(),
        )
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let _ = resp.bytes().await?;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let list: Value = client
            .get(format!("{inspect_url}/api/requests"))
            .send()
            .await?
            .json()
            .await?;
        if let Some(id) = list
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|r| r["id"].as_u64())
        {
            return Ok(id);
        }
        if std::time::Instant::now() >= deadline {
            anyhow::bail!("no request id in list within 5s; last list={list}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

pub(super) async fn poll_detail_for_complete(
    client: &reqwest::Client,
    inspect_url: &str,
    id: u64,
    expect_diagnostics: bool,
) -> anyhow::Result<Value> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let detail: Value = client
            .get(format!("{inspect_url}/api/requests/{id}"))
            .send()
            .await?
            .json()
            .await?;
        if detail["status"] == "Complete" {
            if expect_diagnostics {
                if detail["diagnostics"]
                    .as_array()
                    .map(|arr| !arr.is_empty())
                    .unwrap_or(false)
                {
                    return Ok(detail);
                }
            } else {
                return Ok(detail);
            }
        }
        if std::time::Instant::now() > deadline {
            anyhow::bail!("timed out waiting for Complete status: {detail}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

pub(super) async fn start_sse_mock(sse_body: String) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(sse_body.into_bytes(), "text/event-stream"),
        )
        .mount(&server)
        .await;
    server
}

pub(super) async fn start_json_mock(json_body: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(&json_body)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;
    server
}

pub(super) async fn post_non_streaming_and_get_id(
    client: &reqwest::Client,
    proxy_url: &str,
    inspect_url: &str,
) -> anyhow::Result<u64> {
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("content-type", "application/json")
        .header(
            "x-claude-code-session-id",
            "b2c3d4e5-f6a7-8901-bcde-f01234567891",
        )
        .json(&RequestBody::default().content("Hi").max_tokens(64).build())
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let _ = resp.bytes().await?;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let list: Value = client
            .get(format!("{inspect_url}/api/requests"))
            .send()
            .await?
            .json()
            .await?;
        if let Some(id) = list
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|r| r["id"].as_u64())
        {
            return Ok(id);
        }
        if std::time::Instant::now() >= deadline {
            anyhow::bail!("no request id in list within 5s; last list={list}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}
