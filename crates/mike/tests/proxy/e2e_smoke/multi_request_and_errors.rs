use crate::anthropic_fixtures;
use crate::common;

use anthropic_fixtures::{MockResponse, RequestBody};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_e2e_multiple_requests() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(
                    MockResponse::new("msg_multi")
                        .text("Ok")
                        .usage(5, 1)
                        .build(),
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let (proxy_url, inspect_url) = common::start_proxy(&mock_server.uri()).await?;
    let client = reqwest::Client::new();

    for i in 0..3 {
        let resp = client
            .post(format!("{proxy_url}/v1/messages"))
            .header("x-api-key", "sk-test")
            .header("content-type", "application/json")
            .header(
                "x-claude-code-session-id",
                "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11",
            )
            .json(
                &RequestBody::default()
                    .content(format!("Message {i}"))
                    .max_tokens(100)
                    .build(),
            )
            .send()
            .await?;
        assert_eq!(resp.status(), 200);
        let _body = resp.bytes().await?;
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let list_resp = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?;
    let list_body: Value = list_resp.json().await?;
    let requests = list_body
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("expected array"))?;
    assert_eq!(requests.len(), 3, "should have 3 stored requests");

    let stats_resp = client
        .get(format!("{inspect_url}/api/stats"))
        .send()
        .await?;
    let stats: Value = stats_resp.json().await?;
    assert_eq!(stats["request_count"], 3);
    assert!(
        stats.get("store_size").is_none(),
        "store_size must be absent — dropped in Phase D"
    );

    for req in requests {
        let id = req["id"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("expected id"))?;
        let detail_resp = client
            .get(format!("{inspect_url}/api/requests/{id}"))
            .send()
            .await?;
        assert_eq!(detail_resp.status(), 200);
    }

    let missing_resp = client
        .get(format!("{inspect_url}/api/requests/999999"))
        .send()
        .await?;
    assert_eq!(missing_resp.status(), 404);

    Ok(())
}

#[tokio::test]
async fn test_e2e_error_response_pipeline() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    let error_body = json!({
        "type": "error",
        "error": {
            "type": "rate_limit_error",
            "message": "Rate limit exceeded, please try again later"
        }
    });

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(429)
                .set_body_json(&error_body)
                .insert_header("retry-after", "30")
                .insert_header("content-type", "application/json")
                .insert_header("request-id", "req-e2e-error-001"),
        )
        .mount(&mock_server)
        .await;

    let (proxy_url, inspect_url) = common::start_proxy(&mock_server.uri()).await?;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("content-type", "application/json")
        .header(
            "x-claude-code-session-id",
            "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11",
        )
        .json(
            &RequestBody::default()
                .content("This will be rate limited")
                .max_tokens(100)
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
        "retry-after must be forwarded"
    );
    assert_eq!(
        resp.headers()
            .get("request-id")
            .and_then(|v| v.to_str().ok()),
        Some("req-e2e-error-001"),
        "request-id must be forwarded"
    );

    let body: Value = resp.json().await?;
    assert_eq!(body, error_body, "error body must be forwarded verbatim");

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let list_resp = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?;
    let list_body: Value = list_resp.json().await?;
    let requests = list_body
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("expected array"))?;
    assert_eq!(requests.len(), 1, "error request should be stored");

    let req_row = &requests[0];
    assert_eq!(req_row["method"], "POST");
    assert_eq!(req_row["path"], "/v1/messages");

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
async fn test_e2e_fresh_proxy_endpoints() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    let (_proxy_url, inspect_url) = common::start_proxy(&mock_server.uri()).await?;
    let client = reqwest::Client::new();

    let health_resp = client
        .get(format!("{inspect_url}/api/health"))
        .send()
        .await?;
    assert_eq!(health_resp.status(), 200);
    let health_body: Value = health_resp.json().await?;
    assert!(
        health_body["status"].is_string(),
        "health must carry a status; full body: {health_body}"
    );

    let stats_resp = client
        .get(format!("{inspect_url}/api/stats"))
        .send()
        .await?;
    assert_eq!(stats_resp.status(), 200);
    let stats_body: Value = stats_resp.json().await?;
    assert_eq!(stats_body["request_count"], 0);
    assert!(
        stats_body.get("store_size").is_none(),
        "store_size must be absent — dropped in Phase D"
    );

    let list_resp = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?;
    assert_eq!(list_resp.status(), 200);
    let list_body: Value = list_resp.json().await?;
    let requests = list_body
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("expected array"))?;
    assert!(requests.is_empty(), "fresh proxy should have no requests");

    Ok(())
}
