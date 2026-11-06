use crate::db_helpers;

use crate::anthropic_fixtures;

use anthropic_fixtures::{RequestBody, user_message};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_turn_origin_title_sidecar_classified_in_api_response() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    let response_body = json!({
        "id": "msg_title_sidecar",
        "type": "message",
        "role": "assistant",
        "content": [{"type": "text", "text": "{\"title\": \"Fix login bug\"}"}],
        "model": "claude-haiku-4-5",
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 20, "output_tokens": 10}
    });

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

    let request_body = RequestBody::default()
        .model("claude-haiku-4-5")
        .max_tokens(128)
        .system("Generate a concise, sentence-case title for the following conversation. Respond with JSON like {\"title\": \"...\"}.")
        .content("Can you fix the login redirect bug?")
        .build();

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test-title-sidecar")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header(
            "x-claude-code-session-id",
            "ffffffff-ffff-4fff-8fff-ffffffffffff",
        )
        .json(&request_body)
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let list: Value = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?
        .json()
        .await?;
    let rows = list
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("list must be an array"))?;
    assert_eq!(rows.len(), 1, "exactly one request expected");
    let row = &rows[0];

    assert_eq!(
        row["turn_origin"],
        Value::String("title_sidecar".to_string()),
        "haiku + title-gen markers must classify as title_sidecar"
    );

    let id = row["id"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("request id must be u64"))?;
    let detail: Value = client
        .get(format!("{inspect_url}/api/requests/{id}"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        detail["turn_origin"],
        Value::String("title_sidecar".to_string()),
        "title_sidecar must also appear on detail endpoint"
    );

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn test_turn_origin_subagent_classified_in_api_response() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    let response_body = json!({
        "id": "msg_subagent",
        "type": "message",
        "role": "assistant",
        "content": [{"type": "text", "text": "Done."}],
        "model": "claude-opus-4-7",
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 30, "output_tokens": 5}
    });

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

    let session_id = "00000001-0001-4001-8001-000000000001";
    let main_body = RequestBody::default()
        .model("claude-opus-4-7")
        .system("You are Claude Code, Anthropic's official CLI for Claude.\nYou are an interactive agent that helps users with software engineering tasks.")
        .content("Plan the frontend work.")
        .build();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test-subagent")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", session_id)
        .json(&main_body)
        .send()
        .await?;
    assert_eq!(resp.status(), 200);

    let request_body = RequestBody::default()
        .model("claude-opus-4-7")
        .system("You are Claude Code, Anthropic's official CLI for Claude.\nYou are a Frontend Development Expert — a senior engineer who writes production-grade Svelte and TypeScript.")
        .content("Implement the frontend half of the approved plan.")
        .build();

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test-subagent")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", session_id)
        .json(&request_body)
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let list: Value = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?
        .json()
        .await?;
    let rows = list
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("list must be an array"))?;
    assert_eq!(rows.len(), 2, "exactly two requests expected");
    let row = rows
        .iter()
        .max_by_key(|r| r["id"].as_u64().unwrap_or(0))
        .ok_or_else(|| anyhow::anyhow!("rows must be non-empty"))?;

    assert_eq!(
        row["turn_origin"],
        Value::String("subagent".to_string()),
        "CC header + persona override must classify as subagent"
    );

    let id = row["id"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("request id must be u64"))?;
    let detail: Value = client
        .get(format!("{inspect_url}/api/requests/{id}"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        detail["turn_origin"],
        Value::String("subagent".to_string()),
        "subagent must also appear on detail endpoint"
    );

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn test_turn_origin_advisor_sidecar_classified_in_api_response() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    let response_body = json!({
        "id": "msg_advisor_sidecar",
        "type": "message",
        "role": "assistant",
        "content": [{"type": "text", "text": "<block>no</block>"}],
        "model": "claude-opus-4-7",
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 50, "output_tokens": 8}
    });

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

    let request_body = RequestBody::default()
        .model("claude-opus-4-7")
        .max_tokens(64)
        .system("x-anthropic-billing-header: cc_version=2.1.112.78b; cc_entrypoint=cli; cch=184ed;\nYou are a security monitor for autonomous AI coding agents.\n\n## Context\n\nThe agent you are monitoring is an autonomous agent.")
        .messages(json!([
            user_message("seed"),
            user_message("<transcript>\nUser: fix the login redirect bug\n</transcript>")
        ]))
        .build();

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test-advisor-sidecar")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header(
            "x-claude-code-session-id",
            "00000002-0002-4002-8002-000000000002",
        )
        .json(&request_body)
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let list: Value = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?
        .json()
        .await?;
    let rows = list
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("list must be an array"))?;
    assert_eq!(rows.len(), 1, "exactly one request expected");
    let row = &rows[0];

    assert_eq!(
        row["turn_origin"],
        Value::String("advisor_sidecar".to_string()),
        "security-monitor + mc=2 + tiny max_tokens must classify as advisor_sidecar"
    );

    let id = row["id"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("request id must be u64"))?;
    let detail: Value = client
        .get(format!("{inspect_url}/api/requests/{id}"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        detail["turn_origin"],
        Value::String("advisor_sidecar".to_string()),
        "advisor_sidecar must also appear on detail endpoint"
    );

    test_db.cleanup().await?;
    Ok(())
}
