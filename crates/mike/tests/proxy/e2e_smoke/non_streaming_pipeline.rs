use crate::anthropic_fixtures;
use crate::common;

use super::fixtures::{assert_rate_limit_headers_forwarded, rate_limit_headers};
use anthropic_fixtures::RequestBody;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_e2e_non_streaming_pipeline() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    let response_body = json!({
        "id": "msg_e2e_nonstream_001",
        "type": "message",
        "role": "assistant",
        "content": [
            {"type": "text", "text": "Hello, world! This is a non-streaming response."}
        ],
        "model": "claude-sonnet-4-20250514",
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": {
            "input_tokens": 25,
            "output_tokens": 15,
            "cache_creation_input_tokens": 0,
            "cache_read_input_tokens": 0
        }
    });

    let mut template = ResponseTemplate::new(200)
        .set_body_json(&response_body)
        .insert_header("content-type", "application/json");

    for (name, value) in rate_limit_headers() {
        template = template.insert_header(name, value);
    }

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(template)
        .mount(&mock_server)
        .await;

    let (proxy_url, inspect_url) = common::start_proxy(&mock_server.uri()).await?;
    let client = reqwest::Client::new();

    let request_body = RequestBody::default()
        .max_tokens(2048)
        .system("You are a helpful assistant.")
        .content("Say hello world in a non-streaming way")
        .build();

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-ant-test-key-e2e-002")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header(
            "x-claude-code-session-id",
            "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11",
        )
        .json(&request_body)
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
        "non-streaming content-type must be application/json, got: {content_type}"
    );

    assert_rate_limit_headers_forwarded(resp.headers());

    let body: Value = resp.json().await?;
    assert_eq!(body, response_body, "response body must match mock exactly");

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let req_id = assert_stored_summary_and_get_id(&client, &inspect_url).await?;
    assert_detail_reflects_non_streaming_response(&client, &inspect_url, req_id).await?;
    assert_stats_request_count_at_least_one(&client, &inspect_url).await?;
    assert_upstream_auth_headers_forwarded(&mock_server).await?;

    Ok(())
}

async fn assert_stored_summary_and_get_id(
    client: &reqwest::Client,
    inspect_url: &str,
) -> anyhow::Result<u64> {
    let list_resp = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?;
    assert_eq!(list_resp.status(), 200);

    let list_body: Value = list_resp.json().await?;
    let requests = list_body
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("expected requests array"))?;
    assert_eq!(requests.len(), 1, "should have exactly 1 stored request");

    let req_row = &requests[0];
    assert_eq!(req_row["method"], "POST");
    assert_eq!(req_row["path"], "/v1/messages");
    assert_eq!(req_row["model"], "claude-sonnet-4-20250514");
    assert_eq!(req_row["is_streaming"], false);
    assert_eq!(req_row["message_count"], 1);
    assert_eq!(req_row["status"], "Complete");

    req_row["id"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("expected id as u64"))
}

async fn assert_detail_reflects_non_streaming_response(
    client: &reqwest::Client,
    inspect_url: &str,
    req_id: u64,
) -> anyhow::Result<()> {
    let detail_resp = client
        .get(format!("{inspect_url}/api/requests/{req_id}"))
        .send()
        .await?;
    assert_eq!(detail_resp.status(), 200);

    let detail: Value = detail_resp.json().await?;
    assert_eq!(detail["id"], req_id);
    assert_eq!(detail["method"], "POST");
    assert_eq!(detail["model"], "claude-sonnet-4-20250514");
    assert_eq!(detail["is_streaming"], false);
    assert_eq!(detail["response_status"], 200);
    Ok(())
}

async fn assert_stats_request_count_at_least_one(
    client: &reqwest::Client,
    inspect_url: &str,
) -> anyhow::Result<()> {
    let stats_resp = client
        .get(format!("{inspect_url}/api/stats"))
        .send()
        .await?;
    assert_eq!(stats_resp.status(), 200);

    let stats: Value = stats_resp.json().await?;
    let request_count = stats["request_count"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("expected request_count as u64"))?;
    assert!(
        request_count >= 1,
        "request_count should be >= 1, got: {request_count}"
    );
    Ok(())
}

async fn assert_upstream_auth_headers_forwarded(mock_server: &MockServer) -> anyhow::Result<()> {
    let received = mock_server.received_requests().await;
    let received = received
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no requests received by mock server"))?;
    assert_eq!(
        received.len(),
        1,
        "proxy should send exactly 1 upstream request"
    );

    let upstream_req = &received[0];
    assert_eq!(
        upstream_req
            .headers
            .get("x-api-key")
            .map(|v| v.to_str().unwrap_or("")),
        Some("sk-ant-test-key-e2e-002"),
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
    Ok(())
}
