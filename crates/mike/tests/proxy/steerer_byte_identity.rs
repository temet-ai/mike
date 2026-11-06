use crate::anthropic_fixtures;
use crate::common;

use anthropic_fixtures::{MockResponse, RequestBody};
use sha2::{Digest, Sha256};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn steerer_off_non_streaming_body_byte_identical() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    let response_body = MockResponse::new("msg_steerer_test")
        .text("steerer test response")
        .usage(8, 4)
        .build();
    let expected_bytes = serde_json::to_vec(&response_body)?;

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
    let request_body = RequestBody::default()
        .content("hello from steerer test")
        .build();

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test-key")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request_body)
        .send()
        .await?;

    assert_eq!(resp.status(), 200);

    let body_bytes = resp.bytes().await?;

    let mut expected_hasher = Sha256::new();
    expected_hasher.update(&expected_bytes);
    let expected_hash = expected_hasher.finalize();

    let mut actual_hasher = Sha256::new();
    actual_hasher.update(&body_bytes);
    let actual_hash = actual_hasher.finalize();

    assert_eq!(
        actual_hash, expected_hash,
        "response body SHA-256 mismatch — steerer must not corrupt the forward path"
    );

    Ok(())
}

#[tokio::test]
async fn request_body_byte_identical_through_proxy() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    let response_body = MockResponse::new("msg_req_body_test").usage(5, 1).build();

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

    let raw_body =
        b"{\"model\":\"claude-sonnet-4-20250514\",\"max_tokens\":512,\"messages\":[{\"role\":\"user\",\"content\":\"byte identity guard\"}]}";

    let mut client_hasher = Sha256::new();
    client_hasher.update(raw_body);
    let client_hash = client_hasher.finalize();

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test-key")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .body(raw_body.to_vec())
        .send()
        .await?;

    assert_eq!(resp.status(), 200);

    let received = mock_server.received_requests().await;
    let received = received
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no requests received by upstream"))?;
    assert_eq!(received.len(), 1);

    let upstream_body = &received[0].body;
    let mut upstream_hasher = Sha256::new();
    upstream_hasher.update(upstream_body);
    let upstream_hash = upstream_hasher.finalize();

    assert_eq!(
        client_hash, upstream_hash,
        "request body SHA-256 mismatch — proxy must not reserialize request bodies"
    );

    Ok(())
}
