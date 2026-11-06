use crate::filter_proxy;

use crate::anthropic_fixtures;

use super::support::{drop_system_reminder_rule, start_proxy_dropping_system_reminder};
use anthropic_fixtures::{RequestBody, cached_text_block, text_block, user_message};
use filter_proxy::{claude_response_json, session_id, start_proxy_with_rules};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn drops_all_system_reminder_blocks_preserving_cache_control() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(claude_response_json())
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let Some(proxy) = start_proxy_dropping_system_reminder(&mock_server.uri()).await? else {
        return Ok(());
    };

    let client_body = RequestBody::default()
        .max_tokens(100)
        .system(json!([
            text_block("x-anthropic-billing-header: session=27"),
            cached_text_block("You are Claude Code, Anthropic's official CLI for Claude."),
            cached_text_block(
                "You are an interactive agent that helps users with software engineering tasks."
            ),
        ]))
        .messages(json!([user_message(json!([
            text_block("<system-reminder>SessionStart:startup hook success: OK</system-reminder>"),
            text_block("<system-reminder># MCP Server Instructions ...</system-reminder>"),
            text_block("<system-reminder>UserPromptSubmit hook success: OK</system-reminder>"),
            text_block(
                "<system-reminder># claudeMd ... project instructions ...</system-reminder>"
            ),
            cached_text_block("Please explain the filter change."),
        ]))]))
        .build();

    let resp = reqwest::Client::new()
        .post(format!("{}/v1/messages", proxy.proxy_url))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", session_id())
        .json(&client_body)
        .send()
        .await?;

    assert_eq!(resp.status(), 200);

    let received = mock_server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    assert_eq!(received.len(), 1);

    let upstream_body: Value = serde_json::from_slice(&received[0].body)?;

    let system_arr = upstream_body["system"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("system must be array"))?;
    assert_eq!(system_arr.len(), 3, "system[] blocks untouched");
    assert_eq!(system_arr[1]["cache_control"]["type"], "ephemeral");
    assert_eq!(system_arr[2]["cache_control"]["type"], "ephemeral");

    let content_arr = upstream_body["messages"][0]["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("content must be array"))?;
    assert_eq!(
        content_arr.len(),
        1,
        "all 4 <system-reminder> blocks dropped; 1 user block survives"
    );
    assert_eq!(content_arr[0]["text"], "Please explain the filter change.");
    assert_eq!(content_arr[0]["cache_control"]["type"], "ephemeral");

    proxy.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn replace_overwrites_text_preserving_cache_control() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(claude_response_json())
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let Some(proxy) = start_proxy_with_rules(
        &mock_server.uri(),
        &[json!({
            "kind": "text_substring",
            "value": "The following skills are available in this session. Do not enumerate them.",
            "action": "replace",
            "text": "Skills present; invoke by name.",
        })],
    )
    .await?
    else {
        return Ok(());
    };

    let resp = reqwest::Client::new()
        .post(format!("{}/v1/messages", proxy.proxy_url))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", session_id())
        .json(
            &RequestBody::default()
                .max_tokens(100)
                .system(json!([cached_text_block(
                    "The following skills are available in this session. Do not enumerate them."
                )]))
                .content("ping")
                .build(),
        )
        .send()
        .await?;
    assert_eq!(resp.status(), 200);

    let received = mock_server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    let body: Value = serde_json::from_slice(&received[0].body)?;
    let block = &body["system"][0];
    assert_eq!(block["type"], "text");
    assert_eq!(block["text"], "Skills present; invoke by name.");
    assert_eq!(
        block["cache_control"]["type"], "ephemeral",
        "replace must preserve cache_control on the block"
    );

    proxy.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn append_inserts_block_without_touching_original() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(claude_response_json())
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let Some(proxy) = start_proxy_with_rules(
        &mock_server.uri(),
        &[json!({
            "kind": "text_substring",
            "value": "You are an interactive agent running in the user's terminal.",
            "action": "append",
            "text": "House rules: prefer tool calls.",
        })],
    )
    .await?
    else {
        return Ok(());
    };

    let resp = reqwest::Client::new()
        .post(format!("{}/v1/messages", proxy.proxy_url))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", session_id())
        .json(
            &RequestBody::default()
                .max_tokens(100)
                .system(json!([
                    cached_text_block(
                        "You are an interactive agent running in the user's terminal."
                    ),
                    text_block("Another block."),
                ]))
                .content("ping")
                .build(),
        )
        .send()
        .await?;
    assert_eq!(resp.status(), 200);

    let received = mock_server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    let body: Value = serde_json::from_slice(&received[0].body)?;
    let arr = body["system"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("system must be array"))?;

    assert_eq!(arr.len(), 3, "one block appended after matched block");
    assert_eq!(
        arr[0]["text"],
        "You are an interactive agent running in the user's terminal."
    );
    assert_eq!(arr[0]["cache_control"]["type"], "ephemeral");
    assert_eq!(arr[1]["type"], "text");
    assert_eq!(arr[1]["text"], "House rules: prefer tool calls.");
    assert!(
        arr[1].get("cache_control").is_none(),
        "appended block must not carry cache_control"
    );
    assert_eq!(arr[2]["text"], "Another block.");

    proxy.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn drop_rule_shadows_later_append_rule_on_same_block() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(claude_response_json())
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let Some(proxy) = start_proxy_with_rules(
        &mock_server.uri(),
        &[
            drop_system_reminder_rule(),
            json!({
                "kind": "text_substring",
                "value": "reminder",
                "action": "append",
                "text": "SHOULD NOT RUN",
            }),
        ],
    )
    .await?
    else {
        return Ok(());
    };

    let resp = reqwest::Client::new()
        .post(format!("{}/v1/messages", proxy.proxy_url))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", session_id())
        .json(
            &RequestBody::default()
                .max_tokens(100)
                .system(json!([text_block(
                    "<system-reminder>noise</system-reminder>"
                )]))
                .content("ping")
                .build(),
        )
        .send()
        .await?;
    assert_eq!(resp.status(), 200);

    let received = mock_server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    let body: Value = serde_json::from_slice(&received[0].body)?;
    let arr = body["system"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("system must be array"))?;
    assert!(arr.is_empty(), "drop won; append must not have fired");
    let serialized = serde_json::to_string(&body)?;
    assert!(
        !serialized.contains("SHOULD NOT RUN"),
        "append must not have fired; text leaked: {serialized}"
    );

    proxy.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn messages_content_blocks_dropped_not_just_system() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(claude_response_json())
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let Some(proxy) = start_proxy_dropping_system_reminder(&mock_server.uri()).await? else {
        return Ok(());
    };

    let resp = reqwest::Client::new()
        .post(format!("{}/v1/messages", proxy.proxy_url))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", session_id())
        .json(
            &RequestBody::default()
                .max_tokens(100)
                .messages(json!([user_message(json!([
                    text_block("<system-reminder>drop1</system-reminder>"),
                    text_block("<system-reminder>drop2</system-reminder>"),
                    text_block("real question"),
                ]))]))
                .build(),
        )
        .send()
        .await?;
    assert_eq!(resp.status(), 200);

    let received = mock_server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    let body: Value = serde_json::from_slice(&received[0].body)?;
    let arr = body["messages"][0]["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("content must be array"))?;
    assert_eq!(arr.len(), 1, "both reminders dropped");
    assert_eq!(arr[0]["text"], "real question");

    proxy.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn string_shaped_system_is_never_touched() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(claude_response_json())
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let Some(proxy) = start_proxy_dropping_system_reminder(&mock_server.uri()).await? else {
        return Ok(());
    };

    let client_body = RequestBody::default()
        .max_tokens(100)
        .system("You are a helpful <system-reminder>BUT</system-reminder> assistant.")
        .content("ping")
        .build();
    let client_body_bytes = serde_json::to_vec(&client_body)?;

    let resp = reqwest::Client::new()
        .post(format!("{}/v1/messages", proxy.proxy_url))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", session_id())
        .body(client_body_bytes.clone())
        .send()
        .await?;
    assert_eq!(resp.status(), 200);

    let received = mock_server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    assert_eq!(
        received[0].body, client_body_bytes,
        "string-shaped system[] must never be rewritten"
    );

    proxy.cleanup().await?;
    Ok(())
}
