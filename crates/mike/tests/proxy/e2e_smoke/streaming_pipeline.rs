use crate::anthropic_fixtures;
use crate::common;

use super::fixtures::{
    assert_rate_limit_headers_forwarded, rate_limit_headers, realistic_sse_sequence,
};
use anthropic_fixtures::RequestBody;
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_e2e_full_pipeline() -> anyhow::Result<()> {
    let (raw_sse, _expected_text, _events) = realistic_sse_sequence()?;

    let mock_server = MockServer::start().await;

    let mut template =
        ResponseTemplate::new(200).set_body_raw(raw_sse.as_bytes(), "text/event-stream");
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
        .max_tokens(4096)
        .system("You are a helpful assistant.")
        .content("Say hello world")
        .stream(true)
        .build();

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-ant-test-key-e2e-001")
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
        content_type.starts_with("text/event-stream"),
        "content-type must be text/event-stream, got: {content_type}"
    );

    assert_rate_limit_headers_forwarded(resp.headers());

    let body_bytes = resp.bytes().await?;
    let body_str = String::from_utf8_lossy(&body_bytes);

    let (expected_sse, _, _) = realistic_sse_sequence()?;
    assert_sse_body_byte_identical_and_ordered(&body_str, &expected_sse);

    let (status, req_id) = poll_for_complete_status(&client, &inspect_url).await?;
    assert_eq!(status, "Complete", "request should reach Complete status");

    assert_stored_summary_and_detail(&client, &inspect_url, req_id).await?;
    assert_stats_request_count_at_least_one(&client, &inspect_url).await?;

    Ok(())
}

fn assert_sse_body_byte_identical_and_ordered(body_str: &str, expected_sse: &str) {
    assert_eq!(
        body_str, expected_sse,
        "SSE body must be byte-identical to upstream"
    );

    let expected_event_types = [
        "message_start",
        "content_block_start",
        "ping",
        "content_block_delta",
        "content_block_delta",
        "content_block_delta",
        "content_block_stop",
        "message_delta",
        "message_stop",
    ];
    let received_event_types: Vec<&str> = body_str
        .lines()
        .filter_map(|line| line.strip_prefix("event: "))
        .collect();
    assert_eq!(
        received_event_types, expected_event_types,
        "SSE event types must arrive in order"
    );
}

async fn poll_for_complete_status(
    client: &reqwest::Client,
    inspect_url: &str,
) -> anyhow::Result<(String, u64)> {
    let mut status = String::new();
    let mut req_id: u64 = 0;
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let list_resp = client
            .get(format!("{inspect_url}/api/requests"))
            .send()
            .await?;
        let list_body: Value = list_resp.json().await?;
        if let Some(requests) = list_body.as_array()
            && let Some(first) = requests.first()
        {
            status = first["status"].as_str().unwrap_or("unknown").to_string();
            req_id = first["id"].as_u64().unwrap_or(0);
            if status == "Complete" {
                break;
            }
        }
    }
    Ok((status, req_id))
}

async fn assert_stored_summary_and_detail(
    client: &reqwest::Client,
    inspect_url: &str,
    req_id: u64,
) -> anyhow::Result<()> {
    assert_list_row_fields(client, inspect_url).await?;
    assert_detail_completion(client, inspect_url, req_id).await?;
    Ok(())
}

async fn assert_list_row_fields(client: &reqwest::Client, inspect_url: &str) -> anyhow::Result<()> {
    let list_resp = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?;
    let list_body: Value = list_resp.json().await?;
    let requests = list_body
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("expected requests array"))?;
    assert_eq!(requests.len(), 1, "should have exactly 1 stored request");

    let req_row = &requests[0];
    assert_eq!(req_row["method"], "POST");
    assert_eq!(req_row["path"], "/v1/messages");
    assert_eq!(req_row["model"], "claude-sonnet-4-20250514");
    assert_eq!(req_row["is_streaming"], true);
    assert_eq!(req_row["message_count"], 1);
    Ok(())
}

async fn assert_detail_completion(
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
    assert_eq!(detail["is_streaming"], true);
    assert_eq!(detail["response_status"], 200);
    assert_eq!(detail["stop_reason"], "end_turn");
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
