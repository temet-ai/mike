use crate::db_helpers;

use crate::anthropic_fixtures;

use anthropic_fixtures::{MockResponse, RequestBody};
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_list_requests_after_proxy() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    let response_body = MockResponse::new("msg_test_list").text("Hello!").build();

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(&response_body)
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let proxy = db_helpers::start_proxy_with_db(&mock_server.uri())
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!("MIKE_TEST_ADMIN_URL must be set; DB is required for this test")
        })?;
    let (proxy_url, inspect_url, _shutdown, _pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test-key")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header(
            "x-claude-code-session-id",
            "d4e5f6a7-b8c9-0123-defa-123456789013",
        )
        .json(&RequestBody::default().build())
        .send()
        .await?;

    assert_eq!(resp.status(), 200);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let list_resp = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?;

    assert_eq!(list_resp.status(), 200);
    let list_body: Value = list_resp.json().await?;
    let requests = list_body
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("expected array"))?;

    assert_eq!(requests.len(), 1, "should have 1 recorded request");

    let req = &requests[0];
    assert_eq!(req["method"], "POST");
    assert_eq!(req["path"], "/v1/messages");
    assert_eq!(req["model"], "claude-sonnet-4-20250514");
    assert_eq!(req["is_streaming"], false);
    assert_eq!(req["message_count"], 1);
    assert_eq!(req["status"], "Complete");

    let stats_resp = client
        .get(format!("{inspect_url}/api/stats"))
        .send()
        .await?;
    let stats_body: Value = stats_resp.json().await?;
    assert_eq!(stats_body["request_count"], 1);
    let project_count = stats_body["project_count"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("expected project_count as i64, got: {stats_body}"))?;
    assert_eq!(
        project_count, 0,
        "project_count should be 0 — no session was registered and the sentinel is excluded, got: {project_count}"
    );

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn test_get_request_by_id() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(
                    MockResponse::new("msg_detail")
                        .text("Detail!")
                        .usage(5, 2)
                        .build(),
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let proxy = db_helpers::start_proxy_with_db(&mock_server.uri())
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!("MIKE_TEST_ADMIN_URL must be set; DB is required for this test")
        })?;
    let (proxy_url, inspect_url, _shutdown, _pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test-key")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header(
            "x-claude-code-session-id",
            "e5f6a7b8-c9d0-1234-efab-234567890124",
        )
        .json(&RequestBody::default().build())
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let _ = resp.bytes().await?;
    let id = poll_first_request_id(&client, &inspect_url).await?;

    let detail = fetch_and_assert_detail_shape(&client, &inspect_url, id).await?;
    assert_body_endpoints_reflect_posted_and_returned_bytes(&client, &inspect_url, id, &detail)
        .await?;

    test_db.cleanup().await?;
    Ok(())
}

async fn poll_first_request_id(client: &reqwest::Client, inspect_url: &str) -> anyhow::Result<u64> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let requests = loop {
        let list_resp = client
            .get(format!("{inspect_url}/api/requests"))
            .send()
            .await?;
        let list_body: Value = list_resp.json().await?;
        let arr = list_body
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("expected array"))?
            .clone();
        if !arr.is_empty() || std::time::Instant::now() >= deadline {
            break arr;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    };
    requests[0]["id"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("expected id as u64"))
}

async fn fetch_and_assert_detail_shape(
    client: &reqwest::Client,
    inspect_url: &str,
    id: u64,
) -> anyhow::Result<Value> {
    let detail_resp = client
        .get(format!("{inspect_url}/api/requests/{id}"))
        .send()
        .await?;

    assert_eq!(detail_resp.status(), 200);
    let detail: Value = detail_resp.json().await?;
    assert_eq!(detail["id"], id);
    assert_eq!(detail["method"], "POST");
    assert_eq!(detail["model"], "claude-sonnet-4-20250514");
    assert_eq!(detail["status"], "Complete");
    assert_eq!(detail["response_status"], 200);
    assert!(detail.get("stop_reason").is_some());

    for forbidden in [
        "request_body_text",
        "response_body_text",
        "has_request_body",
        "has_response_body",
        "body_truncated",
        "request_body_truncated",
        "response_body_truncated",
        "text_truncated",
        "thinking_truncated",
        "tool_input_truncated",
    ] {
        assert!(
            detail.get(forbidden).is_none(),
            "detail must not expose `{forbidden}` in the new shape: {detail}"
        );
    }

    let request_body_size = detail["request_body_size"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("request_body_size must be a u64 on detail"))?;
    assert!(
        request_body_size > 0,
        "request body was posted and must have non-zero byte size"
    );
    let response_body_size = detail["response_body_size"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("response_body_size must be a u64 on detail"))?;
    assert!(
        response_body_size > 0,
        "non-streaming response body must have non-zero byte size"
    );

    Ok(detail)
}

async fn assert_body_endpoints_reflect_posted_and_returned_bytes(
    client: &reqwest::Client,
    inspect_url: &str,
    id: u64,
    detail: &Value,
) -> anyhow::Result<()> {
    let request_body_size = detail["request_body_size"].as_u64();
    let response_body_size = detail["response_body_size"].as_u64();

    assert_request_body_endpoint(client, inspect_url, id, request_body_size).await?;
    assert_body_index_links(client, inspect_url, id).await?;
    assert_response_body_endpoint(client, inspect_url, id, response_body_size).await?;
    Ok(())
}

async fn assert_request_body_endpoint(
    client: &reqwest::Client,
    inspect_url: &str,
    id: u64,
    expected_byte_size: Option<u64>,
) -> anyhow::Result<()> {
    let req_body: Value = client
        .get(format!("{inspect_url}/api/requests/{id}/body/request"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(req_body["has_body"], Value::Bool(true));
    assert!(
        req_body.get("truncated").is_none(),
        "BodyPayload no longer carries a `truncated` field: {req_body}"
    );
    assert_eq!(req_body["byte_size"].as_u64(), expected_byte_size);
    let req_text = req_body["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("request body text must decode as UTF-8"))?;
    assert!(
        req_text.contains("\"claude-sonnet-4-20250514\""),
        "request body text must contain the model we posted: {req_text}"
    );
    Ok(())
}

async fn assert_body_index_links(
    client: &reqwest::Client,
    inspect_url: &str,
    id: u64,
) -> anyhow::Result<()> {
    let body_index: Value = client
        .get(format!("{inspect_url}/api/requests/{id}/body"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(body_index["request_id"].as_u64(), Some(id));
    assert_eq!(
        body_index["request"]["href"].as_str(),
        Some(format!("/api/requests/{id}/body/request").as_str())
    );
    assert_eq!(
        body_index["request_sent"]["href"].as_str(),
        Some(format!("/api/requests/{id}/body/request_sent").as_str())
    );
    assert_eq!(
        body_index["response"]["href"].as_str(),
        Some(format!("/api/requests/{id}/body/response").as_str())
    );
    assert_eq!(
        body_index["find"]["href"].as_str(),
        Some(format!("/api/requests/{id}/body/find").as_str())
    );
    Ok(())
}

async fn assert_response_body_endpoint(
    client: &reqwest::Client,
    inspect_url: &str,
    id: u64,
    expected_byte_size: Option<u64>,
) -> anyhow::Result<()> {
    let resp_body: Value = client
        .get(format!("{inspect_url}/api/requests/{id}/body/response"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(resp_body["has_body"], Value::Bool(true));
    assert_eq!(resp_body["byte_size"].as_u64(), expected_byte_size);
    let resp_text = resp_body["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("response body text must decode as UTF-8"))?;
    assert!(
        resp_text.contains("\"msg_detail\""),
        "response body text must contain the upstream message id: {resp_text}"
    );
    Ok(())
}

#[tokio::test]
async fn test_list_endpoint_never_exposes_bodies() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(
                    MockResponse::new("msg_should_not_leak")
                        .text("Ok")
                        .usage(1, 1)
                        .build(),
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let proxy = db_helpers::start_proxy_with_db(&mock_server.uri())
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!("MIKE_TEST_ADMIN_URL must be set; DB is required for this test")
        })?;
    let (proxy_url, inspect_url, _shutdown, _pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("content-type", "application/json")
        .json(
            &RequestBody::default()
                .content("REQUEST_BODY_MARKER_UNIQUE_STRING")
                .max_tokens(1)
                .build(),
        )
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let list_text = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?
        .text()
        .await?;

    for forbidden in [
        "msg_should_not_leak",
        "request_body_text",
        "response_body_text",
    ] {
        assert!(
            !list_text.contains(forbidden),
            "list endpoint leaked `{forbidden}`: {list_text}"
        );
    }

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn test_get_request_not_found() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    let proxy = db_helpers::start_proxy_with_db(&mock_server.uri())
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!("MIKE_TEST_ADMIN_URL must be set; DB is required for this test")
        })?;
    let (_proxy_url, inspect_url, _shutdown, _pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{inspect_url}/api/requests/99999"))
        .send()
        .await?;

    assert_eq!(resp.status(), 404);

    test_db.cleanup().await?;
    Ok(())
}
