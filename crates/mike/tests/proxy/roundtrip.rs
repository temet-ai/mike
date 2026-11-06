use crate::anthropic_fixtures;
use crate::common;

use anthropic_fixtures::{MockResponse, RequestBody};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_proxy_roundtrip_non_streaming() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    let response_body = MockResponse::new("msg_test123").text("Hello!").build();

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(&response_body)
                .insert_header("content-type", "application/json")
                .insert_header("request-id", "req-abc123"),
        )
        .mount(&mock_server)
        .await;

    let (proxy_url, _inspect_url) = common::start_proxy(&mock_server.uri()).await?;

    let client = reqwest::Client::new();
    let request_body = RequestBody::default().build();

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test-key")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request_body)
        .send()
        .await?;

    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("request-id")
            .and_then(|v| v.to_str().ok()),
        Some("req-abc123")
    );

    let body: Value = resp.json().await?;
    assert_eq!(body, response_body);

    let received = mock_server.received_requests().await;
    let received = received
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no requests received by mock server"))?;
    assert_eq!(received.len(), 1, "proxy should send exactly 1 request");

    let upstream_req = &received[0];
    assert_eq!(
        upstream_req
            .headers
            .get("x-api-key")
            .map(|v| v.to_str().unwrap_or("")),
        Some("sk-test-key"),
        "x-api-key must be forwarded"
    );
    assert_eq!(
        upstream_req
            .headers
            .get("anthropic-version")
            .map(|v| v.to_str().unwrap_or("")),
        Some("2023-06-01"),
        "anthropic-version must be forwarded"
    );

    let host = upstream_req
        .headers
        .get("host")
        .map(|v| v.to_str().unwrap_or(""));
    assert!(host.is_some(), "host header should be set");
    let host_val = host.unwrap_or_default();
    assert!(
        host_val.starts_with("127.0.0.1:"),
        "host should be rewritten to upstream authority, got: {host_val}"
    );

    Ok(())
}

#[tokio::test]
async fn test_error_status_passthrough_429() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    let error_body = json!({
        "type": "error",
        "error": {
            "type": "rate_limit_error",
            "message": "Rate limit exceeded"
        }
    });

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(429)
                .set_body_json(&error_body)
                .insert_header("retry-after", "30")
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
        .json(
            &RequestBody::default()
                .messages(json!([]))
                .max_tokens(1)
                .build(),
        )
        .send()
        .await?;

    assert_eq!(resp.status(), 429, "429 must be forwarded verbatim");
    assert_eq!(
        resp.headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok()),
        Some("30"),
        "retry-after header must be forwarded"
    );

    let body: Value = resp.json().await?;
    assert_eq!(
        body, error_body,
        "error body must be forwarded byte-identical"
    );

    let received = mock_server.received_requests().await;
    let received = received
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    assert_eq!(
        received.len(),
        1,
        "proxy must NOT retry — exactly 1 upstream request"
    );

    Ok(())
}

#[tokio::test]
async fn test_error_status_passthrough_500() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    let error_body = json!({
        "type": "error",
        "error": {
            "type": "api_error",
            "message": "Internal server error"
        }
    });

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(500)
                .set_body_json(&error_body)
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
        .json(
            &RequestBody::default()
                .messages(json!([]))
                .max_tokens(1)
                .build(),
        )
        .send()
        .await?;

    assert_eq!(resp.status(), 500, "500 must be forwarded verbatim");

    let body: Value = resp.json().await?;
    assert_eq!(body, error_body);

    Ok(())
}

#[tokio::test]
async fn test_request_body_byte_identical() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"ok": true}))
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let (proxy_url, _inspect_url) = common::start_proxy(&mock_server.uri()).await?;
    let client = reqwest::Client::new();

    let raw_body = r#"{  "model" :   "claude-sonnet-4-20250514" ,
  "max_tokens":1024,
  "messages"  : [ { "role":"user"  , "content" :  "Hello     world" } ],
  "stream":false ,
  "metadata":{"user_id":"test-user-123"}
}"#;

    let sent_hash = {
        let mut hasher = Sha256::new();
        hasher.update(raw_body.as_bytes());
        hasher.finalize()
    };

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .body(raw_body.to_string())
        .send()
        .await?;

    assert_eq!(resp.status(), 200);

    let received = mock_server.received_requests().await;
    let received = received
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    assert_eq!(received.len(), 1);

    let upstream_body = &received[0].body;
    let received_hash = {
        let mut hasher = Sha256::new();
        hasher.update(upstream_body);
        hasher.finalize()
    };

    assert_eq!(
        sent_hash, received_hash,
        "SHA-256 mismatch: proxy reserialized the body!\nSent:     {:x?}\nReceived: {:x?}",
        sent_hash, received_hash
    );

    assert_eq!(
        upstream_body,
        raw_body.as_bytes(),
        "body bytes must be identical — proxy must NOT reserialize"
    );

    Ok(())
}

#[tokio::test]
async fn test_hop_by_hop_headers_stripped() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"ok": true}))
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let (proxy_url, _inspect_url) = common::start_proxy(&mock_server.uri()).await?;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("connection", "keep-alive")
        .header("content-type", "application/json")
        .json(
            &RequestBody::default()
                .messages(json!([]))
                .max_tokens(1)
                .build(),
        )
        .send()
        .await?;

    assert_eq!(resp.status(), 200);

    let received = mock_server.received_requests().await;
    let received = received
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    assert_eq!(received.len(), 1);

    let upstream_req = &received[0];

    assert_eq!(
        upstream_req
            .headers
            .get("anthropic-version")
            .map(|v| v.to_str().unwrap_or("")),
        Some("2023-06-01"),
        "anthropic-version must be forwarded"
    );
    assert_eq!(
        upstream_req
            .headers
            .get("x-api-key")
            .map(|v| v.to_str().unwrap_or("")),
        Some("sk-test"),
        "x-api-key must be forwarded"
    );

    Ok(())
}
