use crate::db_helpers;

use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn v2_request_is_captured_byte_identical() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v2/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"ok": true}))
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

    let raw_body = r#"{"unversioned_field":"probe","nested":{"a":1}}"#;

    let resp = client
        .post(format!("{proxy_url}/v2/messages"))
        .header("x-api-key", "sk-test-key")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header(
            "x-claude-code-session-id",
            "f6a7b8c9-d0e1-2345-fabc-345678901235",
        )
        .body(raw_body.to_string())
        .send()
        .await?;

    assert_eq!(resp.status(), 200, "/v2/* must be accepted and forwarded");

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
    assert_eq!(requests.len(), 1, "should have 1 recorded /v2 request");
    assert_eq!(requests[0]["path"], "/v2/messages");
    let id = requests[0]["id"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("expected id as u64"))?;

    let body_resp = client
        .get(format!("{inspect_url}/api/requests/{id}/body/request"))
        .send()
        .await?;
    assert_eq!(body_resp.status(), 200);
    let body_payload: Value = body_resp.json().await?;
    assert_eq!(
        body_payload["text"].as_str(),
        Some(raw_body),
        "captured request body must be byte-identical to what was sent"
    );

    test_db.cleanup().await?;
    Ok(())
}
