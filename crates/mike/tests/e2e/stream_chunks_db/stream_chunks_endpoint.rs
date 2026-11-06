use crate::anthropic_fixtures;
use crate::db_helpers;
use anthropic_fixtures::{MockResponse, RequestBody};
use db_helpers::start_proxy_with_db;
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{fire_streaming_request_and_get_id, sha256_hex, sse_streaming_body, wait_for_complete};

async fn wait_for_stream_blocks(
    client: &reqwest::Client,
    inspect_url: &str,
    id: u64,
) -> anyhow::Result<serde_json::Value> {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let payload: serde_json::Value = client
            .get(format!("{inspect_url}/api/requests/{id}/stream_chunks"))
            .send()
            .await?
            .json()
            .await?;
        let written = payload["stream_blocks"]
            .as_array()
            .is_some_and(|blocks| !blocks.is_empty());
        if written {
            return Ok(payload);
        }
        if std::time::Instant::now() > deadline {
            anyhow::bail!("stream_blocks for request {id} still empty after 30s: {payload}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn get_stream_chunks_returns_one_coalesced_block_for_streaming_request() -> anyhow::Result<()>
{
    let mock_server = MockServer::start().await;

    let chunks = ["Hello ", "beautiful ", "world!"];
    let sse_body = sse_streaming_body(&chunks);

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(sse_body.as_bytes(), "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let client = reqwest::Client::new();

    let id =
        fire_streaming_request_and_get_id(&client, &proxy.proxy_url, &proxy.inspect_url).await?;

    wait_for_complete(&client, &proxy.inspect_url, id, Duration::from_secs(30)).await?;

    let payload = wait_for_stream_blocks(&client, &proxy.inspect_url, id).await?;

    assert_eq!(
        payload["request_id"].as_u64(),
        Some(id),
        "envelope request_id must match"
    );
    let block_arr = payload["stream_blocks"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("stream_blocks must be an array"))?;

    assert_eq!(
        block_arr.len(),
        1,
        "three text_deltas must coalesce into one text block: {block_arr:?}"
    );
    let block = &block_arr[0];
    assert_eq!(block["block_type"].as_str(), Some("text"));
    assert_eq!(
        block["content"].as_str(),
        Some("Hello beautiful world!"),
        "the coalesced block must hold the full contiguous text"
    );
    let sha = block["content_sha256"].as_str().unwrap_or("");
    assert_eq!(sha.len(), 64, "content_sha256 must be a 64-char hex string");
    assert_eq!(
        sha,
        sha256_hex("Hello beautiful world!"),
        "content_sha256 must be the sha of the full block content"
    );
    assert!(
        block.get("signature").map(|v| v.is_null()).unwrap_or(true),
        "a text block must not carry a signature"
    );

    proxy.shutdown.cancel();
    proxy.test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn get_stream_chunks_returns_one_synthetic_block_for_non_streaming() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                MockResponse::new("msg_ns")
                    .text("hello")
                    .usage(5, 3)
                    .build(),
            ),
        )
        .mount(&mock_server)
        .await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/v1/messages", proxy.proxy_url))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header(
            "x-claude-code-session-id",
            "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11",
        )
        .json(&RequestBody::default().content("non-streaming").build())
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let _ = resp.bytes().await?;

    let list: serde_json::Value = client
        .get(format!("{}/api/requests", proxy.inspect_url))
        .send()
        .await?
        .json()
        .await?;
    let id = list
        .as_array()
        .and_then(|a| a.first())
        .and_then(|r| r["id"].as_u64())
        .ok_or_else(|| anyhow::anyhow!("no request recorded"))?;

    wait_for_complete(&client, &proxy.inspect_url, id, Duration::from_secs(5)).await?;

    let payload: serde_json::Value = client
        .get(format!(
            "{}/api/requests/{}/stream_chunks",
            proxy.inspect_url, id
        ))
        .send()
        .await?
        .json()
        .await?;

    assert_eq!(payload["request_id"].as_u64(), Some(id));
    let block_arr = payload["stream_blocks"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("stream_blocks must be an array"))?;
    assert_eq!(
        block_arr.len(),
        1,
        "non-streaming request must have exactly one synthetic text block: {block_arr:?}"
    );
    assert_eq!(block_arr[0]["block_type"].as_str(), Some("text"));
    assert_eq!(block_arr[0]["content"].as_str(), Some("hello"));

    proxy.shutdown.cancel();
    proxy.test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn get_stream_chunks_returns_404_for_unknown_request() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let client = reqwest::Client::new();

    let resp = client
        .get(format!(
            "{}/api/requests/999999/stream_chunks",
            proxy.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), 404, "unknown request id must return 404");

    proxy.shutdown.cancel();
    proxy.test_db.cleanup().await?;
    Ok(())
}
