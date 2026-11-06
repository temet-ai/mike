use crate::common;

use crate::db_helpers;

use crate::anthropic_fixtures;

use anthropic_fixtures::RequestBody;
use common::start_proxy;
use std::time::Duration;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn upstream_receives_accept_encoding_identity() -> anyhow::Result<()> {
    let upstream = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("accept-encoding", "identity"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(
                    r#"{"id":"msg_01","type":"message","role":"assistant","content":[],"model":"claude-haiku-4-5","stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}"#,
                ),
        )
        .expect(1)
        .mount(&upstream)
        .await;

    let (proxy_url, _inspect_url) = start_proxy(&upstream.uri()).await?;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("accept-encoding", "gzip, deflate, br, zstd")
        .header("content-type", "application/json")
        .header("x-api-key", "sk-test")
        .body(
            RequestBody::default()
                .model("claude-haiku-4-5")
                .content("hi")
                .max_tokens(1)
                .build()
                .to_string(),
        )
        .send()
        .await?;

    assert_eq!(resp.status(), 200);
    Ok(())
}

async fn poll_first_request_id(
    client: &reqwest::Client,
    inspect_url: &str,
    deadline: std::time::Instant,
) -> anyhow::Result<u64> {
    loop {
        let listing: serde_json::Value = client
            .get(format!("{inspect_url}/api/requests"))
            .send()
            .await?
            .json()
            .await?;
        if let Some(id) = listing
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|first| first.get("id"))
            .and_then(|v| v.as_u64())
        {
            return Ok(id);
        }
        if std::time::Instant::now() >= deadline {
            anyhow::bail!(
                "expected at least one request in listing within 5s; last listing={listing}"
            );
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn poll_until_complete(
    client: &reqwest::Client,
    inspect_url: &str,
    id: u64,
    deadline: std::time::Instant,
) -> anyhow::Result<serde_json::Value> {
    loop {
        let detail: serde_json::Value = client
            .get(format!("{inspect_url}/api/requests/{id}"))
            .send()
            .await?
            .json()
            .await?;
        if detail["status"].as_str() == Some("Complete") && detail["stop_reason"].as_str().is_some()
        {
            return Ok(detail);
        }
        if std::time::Instant::now() >= deadline {
            anyhow::bail!(
                "request {id} never reached Complete+stop_reason within 5s; last detail={detail}"
            );
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[tokio::test]
async fn streaming_response_parses_when_upstream_returns_identity() -> anyhow::Result<()> {
    let upstream = MockServer::start().await;

    let sse = "event: message_start\n\
data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_01\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-haiku-4-5\",\"stop_reason\":null,\"usage\":{\"input_tokens\":3,\"output_tokens\":0}}}\n\n\
event: content_block_start\n\
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hello\"}}\n\n\
event: content_block_stop\n\
data: {\"type\":\"content_block_stop\",\"index\":0}\n\n\
event: message_delta\n\
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":1}}\n\n\
event: message_stop\n\
data: {\"type\":\"message_stop\"}\n\n";

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("accept-encoding", "identity"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(sse.as_bytes(), "text/event-stream"))
        .expect(1)
        .mount(&upstream)
        .await;

    let proxy = db_helpers::start_proxy_with_db(&upstream.uri())
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!("MIKE_TEST_ADMIN_URL must be set; DB is required for this test")
        })?;
    let (proxy_url, inspect_url, _shutdown, _pool, test_db) = proxy.into_parts();

    let client = reqwest::Client::new();
    let mut resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("accept-encoding", "gzip")
        .header("content-type", "application/json")
        .header("x-api-key", "sk-test")
        .header(
            "x-claude-code-session-id",
            "c3d4e5f6-a7b8-9012-cdef-012345678902",
        )
        .body(
            RequestBody::default()
                .model("claude-haiku-4-5")
                .content("hi")
                .max_tokens(10)
                .stream(true)
                .build()
                .to_string(),
        )
        .send()
        .await?;
    assert_eq!(resp.status(), 200);

    while let Some(_chunk) = resp.chunk().await? {}
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let list_deadline = std::time::Instant::now() + Duration::from_secs(5);
    let id = poll_first_request_id(&client, &inspect_url, list_deadline).await?;

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let detail = poll_until_complete(&client, &inspect_url, id, deadline).await?;

    assert_eq!(detail["stop_reason"].as_str(), Some("end_turn"));
    assert_eq!(
        detail["token_usage"]["output_tokens"].as_u64(),
        Some(1),
        "output_tokens must be populated from message_delta usage"
    );
    test_db.cleanup().await?;
    Ok(())
}
