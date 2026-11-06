use crate::db_helpers;

use crate::anthropic_fixtures;

use anthropic_fixtures::{MockResponse, RequestBody};
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn assert_list_row_extractor_fields(row: &Value) -> anyhow::Result<()> {
    assert_eq!(
        row["project"],
        Value::Null,
        "project must stay unresolved — no session registration occurred, and system-prompt content is not project identity"
    );

    let hp = row["human_prompt"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("human_prompt must be a string on list row"))?;
    assert!(
        hp.contains("Explain this function"),
        "human_prompt must contain the real human text, got: {hp}"
    );
    assert!(
        !hp.contains("<system-reminder>"),
        "human_prompt must NOT contain <system-reminder> XML, got: {hp}"
    );
    assert!(
        !hp.contains("injected context"),
        "human_prompt must NOT contain content inside the stripped tag, got: {hp}"
    );

    assert_eq!(
        row["turn_type"],
        Value::String("user_prompt".to_string()),
        "turn_type must be user_prompt for a normal user message"
    );

    assert_eq!(
        row["turn_origin"],
        Value::String("primary_first".to_string()),
        "turn_origin must be primary_first for the first request in a new session"
    );

    assert!(
        row.get("response_preview").is_none(),
        "response_preview must be absent on the list row — dropped in Phase D"
    );

    assert!(
        row["response_tool_count"].is_number(),
        "response_tool_count must be a number on list row"
    );
    assert_eq!(
        row["response_tool_count"].as_u64(),
        Some(0),
        "no tool calls in this response"
    );

    let tools = row["response_tools_used"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("response_tools_used must be an array"))?;
    assert!(tools.is_empty(), "no tools used in this response");
    Ok(())
}

fn assert_detail_extractor_fields(detail: &Value) -> anyhow::Result<()> {
    assert_eq!(
        detail["project"],
        Value::Null,
        "project must stay unresolved on the detail endpoint too"
    );
    let detail_hp = detail["human_prompt"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("human_prompt must be a string on detail"))?;
    assert!(
        detail_hp.contains("Explain this function"),
        "detail human_prompt must contain the real text, got: {detail_hp}"
    );
    assert!(
        !detail_hp.contains("<system-reminder>"),
        "detail human_prompt must strip <system-reminder>, got: {detail_hp}"
    );
    assert_eq!(
        detail["turn_type"],
        Value::String("user_prompt".to_string()),
        "turn_type must appear on detail endpoint"
    );
    assert_eq!(
        detail["turn_origin"],
        Value::String("primary_first".to_string()),
        "turn_origin must appear on detail endpoint (primary_first for first request in session)"
    );
    Ok(())
}

#[tokio::test]
async fn test_phase28_extractor_fields_in_api_response() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    let response_body = MockResponse::new("msg_extractor_wiring")
        .text("Here is the explanation.")
        .usage(20, 10)
        .build();

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
        .system("You are a helpful assistant.\nContents of /Users/testuser/projects/ai/tools/proxy/CLAUDE.md (project instructions, checked into the codebase):\n\n# Claude Proxy\n\nSome instructions here.")
        .content("<system-reminder>some injected context that should be stripped</system-reminder>\nExplain this function")
        .build();

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test-extractor")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header(
            "x-claude-code-session-id",
            "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee",
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
    assert_list_row_extractor_fields(row)?;

    let id = row["id"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("request id must be u64"))?;
    let detail: Value = client
        .get(format!("{inspect_url}/api/requests/{id}"))
        .send()
        .await?
        .json()
        .await?;
    assert_detail_extractor_fields(&detail)?;

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn test_project_ignores_last_user_message_system_reminder_marker() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    let response_body = MockResponse::new("msg_project_from_user_msg")
        .text("ok")
        .usage(5, 2)
        .build();

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

    let user_message = "<system-reminder>\n\
                        # claudeMd\n\
                        Contents of /test/project/CLAUDE.md (project instructions, checked into the codebase):\n\
                        some project instructions\n\
                        </system-reminder>\n\
                        Do the work.";

    let request_body = RequestBody::default()
        .system("You are Claude Code, Anthropic's official CLI for Claude.")
        .content(user_message)
        .build();

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test-project-extract")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header(
            "x-claude-code-session-id",
            "00000003-0003-4003-8003-000000000003",
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
        row["project"],
        Value::Null,
        "project must stay unresolved — request-body markers are not project identity"
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
        detail["project"],
        Value::Null,
        "detail endpoint must mirror the list value — still unresolved"
    );

    test_db.cleanup().await?;
    Ok(())
}
