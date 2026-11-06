use crate::anthropic_fixtures;
use crate::common;

use anthropic_fixtures::{MockResponse, RequestBody};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_rate_limit_header_passthrough() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    let response_body = MockResponse::new("msg_ratelimit")
        .text("Hello")
        .usage(10, 1)
        .build();

    let rate_limit_headers: Vec<(&str, &str)> = vec![
        ("anthropic-ratelimit-requests-limit", "1000"),
        ("anthropic-ratelimit-requests-remaining", "999"),
        ("anthropic-ratelimit-requests-reset", "2025-01-01T00:00:00Z"),
        ("anthropic-ratelimit-tokens-limit", "100000"),
        ("anthropic-ratelimit-tokens-remaining", "99000"),
        ("anthropic-ratelimit-tokens-reset", "2025-01-01T00:00:00Z"),
        ("anthropic-ratelimit-input-tokens-limit", "80000"),
        ("anthropic-ratelimit-input-tokens-remaining", "79000"),
        (
            "anthropic-ratelimit-input-tokens-reset",
            "2025-01-01T00:00:10Z",
        ),
        ("anthropic-ratelimit-output-tokens-limit", "20000"),
        ("anthropic-ratelimit-output-tokens-remaining", "19500"),
        (
            "anthropic-ratelimit-output-tokens-reset",
            "2025-01-01T00:00:20Z",
        ),
        ("retry-after", "42"),
        ("request-id", "req-abc-ratelimit-test"),
    ];

    let mut template = ResponseTemplate::new(200)
        .set_body_json(&response_body)
        .insert_header("content-type", "application/json");

    for (name, value) in &rate_limit_headers {
        template = template.insert_header(*name, *value);
    }

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(template)
        .mount(&mock_server)
        .await;

    let (proxy_url, _inspect_url) = common::start_proxy(&mock_server.uri()).await?;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&RequestBody::default().build())
        .send()
        .await?;

    assert_eq!(resp.status(), 200);

    for (name, expected_value) in &rate_limit_headers {
        let actual = resp
            .headers()
            .get(*name)
            .unwrap_or_else(|| panic!("missing header: {name}"))
            .to_str()
            .unwrap_or_else(|_| panic!("non-string header value for: {name}"));

        assert_eq!(
            actual, *expected_value,
            "header {name}: expected '{expected_value}', got '{actual}'"
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_error_status_passthrough_529() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    let error_body = json!({
        "type": "error",
        "error": {
            "type": "overloaded_error",
            "message": "Overloaded"
        }
    });

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(529)
                .set_body_json(&error_body)
                .insert_header("retry-after", "15")
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

    assert_eq!(resp.status(), 529, "529 must be forwarded verbatim");
    assert_eq!(
        resp.headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok()),
        Some("15"),
        "retry-after must be forwarded"
    );

    let body: serde_json::Value = resp.json().await?;
    assert_eq!(
        body, error_body,
        "529 error body must be forwarded verbatim"
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
async fn test_request_header_capture_cap_drops_overflow() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(MockResponse::new("msg_hcap").text("ok").usage(1, 1).build())
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let (proxy_url, inspect_url) = common::start_proxy(&mock_server.uri()).await?;
    let client = reqwest::Client::new();

    let synth_count = 90usize;
    let mut builder = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header(
            "x-claude-code-session-id",
            "22222222-2222-4222-8222-222222222222",
        )
        .header("content-type", "application/json");
    for i in 0..synth_count {
        builder = builder.header(format!("x-synth-{i:03}"), format!("v{i:03}"));
    }

    let resp = builder
        .json(&RequestBody::default().content("hi").max_tokens(1).build())
        .send()
        .await?;
    assert_eq!(resp.status(), 200, "proxy must forward upstream verbatim");

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let list_resp = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?;
    let list_body: serde_json::Value = list_resp.json().await?;
    let requests = list_body
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("expected array"))?;
    assert!(!requests.is_empty(), "expected one captured request");
    let id = requests[0]["id"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("expected id as u64"))?;

    let detail_resp = client
        .get(format!("{inspect_url}/api/requests/{id}"))
        .send()
        .await?;
    assert_eq!(detail_resp.status(), 200);
    let detail: serde_json::Value = detail_resp.json().await?;
    let captured_headers = detail
        .get("request_headers")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("request_headers missing or not array"))?;

    assert!(
        captured_headers.len() <= 64,
        "request_headers must be capped at MAX_HEADERS = 64, got {}",
        captured_headers.len()
    );

    let received = mock_server.received_requests().await;
    let received = received
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    assert_eq!(received.len(), 1, "exactly one upstream request");
    let upstream_synth_count = received[0]
        .headers
        .iter()
        .filter(|(name, _)| name.as_str().starts_with("x-synth-"))
        .count();
    assert_eq!(
        upstream_synth_count, synth_count,
        "upstream must see all {synth_count} synthetic headers — the cap applies to the captured projection only"
    );

    Ok(())
}

#[tokio::test]
async fn test_request_header_capture_cap_drops_oversize_pair() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(MockResponse::new("msg_hbig").text("ok").usage(1, 1).build())
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let (proxy_url, inspect_url) = common::start_proxy(&mock_server.uri()).await?;
    let client = reqwest::Client::new();

    let huge_value = "v".repeat(9 * 1024);
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header(
            "x-claude-code-session-id",
            "33333333-3333-4333-8333-333333333333",
        )
        .header("content-type", "application/json")
        .header("x-oversize-header", &huge_value)
        .json(&RequestBody::default().content("hi").max_tokens(1).build())
        .send()
        .await?;
    assert_eq!(resp.status(), 200);

    let id = {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let list_body: serde_json::Value = client
                .get(format!("{inspect_url}/api/requests"))
                .send()
                .await?
                .json()
                .await?;
            if let Some(id) = list_body
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|r| r["id"].as_u64())
            {
                break id;
            }
            if std::time::Instant::now() >= deadline {
                anyhow::bail!(
                    "expected at least one request in listing within 5s; last list={list_body}"
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    };

    let detail_resp = client
        .get(format!("{inspect_url}/api/requests/{id}"))
        .send()
        .await?;
    let detail: serde_json::Value = detail_resp.json().await?;
    let captured_headers = detail
        .get("request_headers")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("request_headers missing or not array"))?;

    let oversize_present = captured_headers.iter().any(|pair| {
        pair.as_array()
            .and_then(|arr| arr.first())
            .and_then(|name| name.as_str())
            .is_some_and(|name| name == "x-oversize-header")
    });
    assert!(
        !oversize_present,
        "oversize header pair must be dropped from the captured projection"
    );

    let received = mock_server.received_requests().await;
    let received = received
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    let upstream_saw_oversize = received[0]
        .headers
        .iter()
        .any(|(name, _)| name.as_str() == "x-oversize-header");
    assert!(
        upstream_saw_oversize,
        "upstream must see the oversize header — the cap applies to the captured projection only"
    );

    Ok(())
}
