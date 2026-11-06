use crate::anthropic_fixtures;

use super::harness::{
    lazy_mock_upstream, start_proxy_with_config, start_proxy_with_config_upstream,
};

use anthropic_fixtures::{RequestBody, cached_text_block, text_block};
use serde_json::{Value, json};

const REMINDER_LABEL: &str = "chrome reminder";
const REMINDER_BODY: &str =
    "Use tabs_context_mcp before any other browser tool. Never guess tab ids.";
const REMINDER_MARKER: &str = "<chrome>";

async fn create_inject_reminder_rule(
    client: &reqwest::Client,
    inspect_url: &str,
    enabled: bool,
) -> anyhow::Result<()> {
    let resp = client
        .post(format!("{inspect_url}/api/filter/rules"))
        .json(&json!({
            "kind": "inject_reminder",
            "value": REMINDER_LABEL,
            "action": "replace",
            "text": REMINDER_BODY,
            "trigger_token": REMINDER_MARKER,
            "enabled": enabled,
        }))
        .send()
        .await?;
    anyhow::ensure!(
        resp.status() == 201,
        "inject_reminder create must return 201, got {}: {}",
        resp.status(),
        resp.text().await?
    );
    Ok(())
}

fn reminder_request_body(messages: Value) -> Value {
    RequestBody::default()
        .max_tokens(100)
        .system(json!([
            text_block("billing header"),
            cached_text_block("You are Claude Code. No chrome section here."),
        ]))
        .messages(messages)
        .build()
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn inject_reminder_rule_crud_round_trips_trigger_token() -> anyhow::Result<()> {
    let Some(proxy) = start_proxy_with_config(b"persist_enabled = true\n").await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/filter/rules", proxy.inspect_url))
        .json(&json!({
            "kind": "inject_reminder",
            "value": REMINDER_LABEL,
            "action": "replace",
            "text": REMINDER_BODY,
            "trigger_token": REMINDER_MARKER,
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), 201);
    let envelope: Value = resp.json().await?;
    let dto = &envelope["rule"];
    assert_eq!(dto["kind"], "inject_reminder");
    assert_eq!(dto["trigger_token"], REMINDER_MARKER);
    assert_eq!(dto["text"], REMINDER_BODY);
    assert_eq!(dto["value"], REMINDER_LABEL);
    let id = dto["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("rule id must be a string"))?
        .to_string();

    let rules: Vec<Value> = client
        .get(format!("{}/api/filter/rules", proxy.inspect_url))
        .send()
        .await?
        .json()
        .await?;
    let listed = rules
        .iter()
        .find(|r| r["id"] == json!(id))
        .ok_or_else(|| anyhow::anyhow!("created rule missing from list"))?;
    assert_eq!(listed["trigger_token"], REMINDER_MARKER);
    assert_eq!(listed["kind"], "inject_reminder");

    let bad = client
        .post(format!("{}/api/filter/rules", proxy.inspect_url))
        .json(&json!({
            "kind": "inject_reminder",
            "value": "another reminder",
            "action": "replace",
            "text": "body",
        }))
        .send()
        .await?;
    assert_eq!(bad.status(), 400);
    let err: Value = bad.json().await?;
    assert!(
        err["error"]
            .as_str()
            .unwrap_or("")
            .contains("`trigger_token`"),
        "validator message must surface, got: {err}"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn inject_reminder_trigger_injects_once_per_window_without_system_strip() -> anyhow::Result<()>
{
    let mock_server = lazy_mock_upstream().await;
    let Some(proxy) =
        start_proxy_with_config_upstream(b"persist_enabled = true\n", &mock_server.uri()).await?
    else {
        return Ok(());
    };
    let client = reqwest::Client::new();
    create_inject_reminder_rule(&client, &proxy.inspect_url, true).await?;
    let session = uuid::Uuid::new_v4().to_string();

    let request_a_messages = json!([
        {"role": "user", "content": [{"type": "text", "text": "open the dashboard page"}]},
        {"role": "assistant", "content": [
            {"type": "text", "text": "Loading browser guidance: <chrome>"},
            {"type": "tool_use", "id": "tu_1", "name": "mcp__claude-in-chrome__tabs_context_mcp", "input": {}}
        ]},
        {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "tu_1", "content": "tab group: 1 tab"}
        ]},
    ]);
    let resp_a = client
        .post(format!("{}/v1/messages", proxy.proxy_url))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", &session)
        .json(&reminder_request_body(request_a_messages.clone()))
        .send()
        .await?;
    assert_eq!(resp_a.status(), 200);

    let mut request_b_messages = request_a_messages
        .as_array()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("messages fixture must be an array"))?;
    request_b_messages.push(json!(
        {"role": "assistant", "content": [{"type": "text", "text": "The dashboard tab is open."}]}
    ));
    request_b_messages.push(json!(
        {"role": "user", "content": [{"type": "text", "text": "now click the export button"}]}
    ));
    let resp_b = client
        .post(format!("{}/v1/messages", proxy.proxy_url))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", &session)
        .json(&reminder_request_body(Value::Array(request_b_messages)))
        .send()
        .await?;
    assert_eq!(resp_b.status(), 200);

    let received = mock_server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    assert_eq!(received.len(), 2);
    let upstream_a: Value = serde_json::from_slice(&received[0].body)?;
    let upstream_b: Value = serde_json::from_slice(&received[1].body)?;

    let sys_a = &upstream_a["system"];
    assert_eq!(
        sys_a[0]["text"], "billing header",
        "system block 0 untouched"
    );
    assert_eq!(
        sys_a[1]["text"], "You are Claude Code. No chrome section here.",
        "system block 1 untouched — inject_reminder never strips"
    );
    assert_eq!(
        sys_a[1]["cache_control"]["type"], "ephemeral",
        "cache_control preserved (system block not rewritten)"
    );

    let content_a = upstream_a["messages"][2]["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("messages[2].content must be array"))?;
    assert_eq!(content_a.len(), 2, "tool_result + injected reminder");
    let reminder_a = content_a[1]["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("reminder text must be string"))?;
    assert_eq!(
        reminder_a,
        format!("<chrome>\n{REMINDER_BODY}\n</chrome>"),
        "reminder body wrapped in the marker-derived tag, full-fidelity"
    );

    let serialized_b = serde_json::to_string(&upstream_b)?;
    assert_eq!(
        serialized_b.matches("<chrome>\\n").count(),
        1,
        "once-per-window: no re-injection once the marker is no longer the latest assistant turn"
    );
    let content_b = upstream_b["messages"][2]["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("messages[2].content must be array"))?;
    assert_eq!(
        content_b, content_a,
        "the injected block replays byte for byte where it was injected, so the cached prefix holds"
    );
    let newest_b = upstream_b["messages"][4]["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("messages[4].content must be array"))?;
    assert_eq!(newest_b.len(), 1, "the newest user turn gets no reminder");
    assert_eq!(
        upstream_b["system"][1]["text"], "You are Claude Code. No chrome section here.",
        "request B system prompt still untouched"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn inject_reminder_disabled_forwards_byte_identical() -> anyhow::Result<()> {
    let mock_server = lazy_mock_upstream().await;
    let Some(proxy) =
        start_proxy_with_config_upstream(b"persist_enabled = true\n", &mock_server.uri()).await?
    else {
        return Ok(());
    };
    let client = reqwest::Client::new();
    create_inject_reminder_rule(&client, &proxy.inspect_url, false).await?;

    let body = serde_json::to_vec(&reminder_request_body(json!([
        {"role": "assistant", "content": [{"type": "text", "text": "go <chrome>"}]},
        {"role": "user", "content": [{"type": "text", "text": "next"}]}
    ])))?;
    let resp = client
        .post(format!("{}/v1/messages", proxy.proxy_url))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .body(body.clone())
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let received = mock_server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    let last = received
        .last()
        .ok_or_else(|| anyhow::anyhow!("expected an upstream request"))?;
    assert_eq!(
        last.body, body,
        "disabled inject_reminder: upstream body must be byte-identical"
    );
    Ok(())
}
