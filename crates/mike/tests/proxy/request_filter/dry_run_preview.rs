use crate::common;

use crate::filter_proxy;

use crate::anthropic_fixtures;

use anthropic_fixtures::{RequestBody, text_block, user_message};
use filter_proxy::{claude_response_json, session_id};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn capture_request_with_reminder(
    client: &reqwest::Client,
    proxy_url: &str,
    inspect_url: &str,
) -> anyhow::Result<u64> {
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", session_id())
        .json(
            &RequestBody::default()
                .max_tokens(100)
                .system(json!([
                    text_block("Preamble."),
                    text_block("<system-reminder>SECRET</system-reminder>"),
                ]))
                .messages(json!([user_message(json!([text_block("hi")]))]))
                .build(),
        )
        .send()
        .await?;
    assert_eq!(resp.status(), 200);

    let requests: Value = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?
        .json()
        .await?;
    requests[0]["id"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("request id must be a number"))
}

async fn assert_dry_run_preview_drops_reminder(
    client: &reqwest::Client,
    inspect_url: &str,
    request_id: u64,
) -> anyhow::Result<()> {
    let dry_resp = client
        .post(format!("{inspect_url}/api/filter/rules/dry-run"))
        .json(&json!({
            "request_id": request_id,
            "rules": [
                {"kind": "xml_tag", "value": "system-reminder", "action": "drop"}
            ]
        }))
        .send()
        .await?;
    assert_eq!(dry_resp.status(), 200);
    let preview: Value = dry_resp.json().await?;

    assert_eq!(preview["changed"], true, "draft drop rule must fire");
    assert_eq!(
        preview["firing_rule_ids"],
        json!([-1]),
        "a draft rule with no `id` reports its negated 1-based position, never a real rule id"
    );
    let original_len = preview["original_len"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("original_len must be a number"))?;
    let filtered_len = preview["filtered_len"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("filtered_len must be a number"))?;
    assert!(
        filtered_len < original_len,
        "dropping a block must shrink the body: {filtered_len} >= {original_len}"
    );
    let sys = preview["filtered_system"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("filtered_system must be the full system array"))?;
    assert_eq!(sys.len(), 1, "reminder block dropped from preview");
    assert_eq!(sys[0]["text"], "Preamble.");
    assert!(
        !serde_json::to_string(&preview)?.contains("SECRET"),
        "dropped content must not appear in the filtered subtrees"
    );
    let msgs = preview["filtered_messages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("filtered_messages must be the full messages array"))?;
    assert_eq!(msgs[0]["content"][0]["text"], "hi");
    Ok(())
}

async fn assert_no_rules_persisted(
    client: &reqwest::Client,
    inspect_url: &str,
) -> anyhow::Result<()> {
    let rules: Vec<Value> = client
        .get(format!("{inspect_url}/api/filter/rules"))
        .send()
        .await?
        .json()
        .await?;
    assert!(
        rules.is_empty(),
        "dry-run must not persist draft rules, got: {rules:?}"
    );
    Ok(())
}

async fn assert_second_request_is_unfiltered(
    client: &reqwest::Client,
    proxy_url: &str,
    mock_server: &MockServer,
) -> anyhow::Result<()> {
    let resp2 = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(
            &RequestBody::default()
                .max_tokens(100)
                .system(json!([text_block(
                    "<system-reminder>SECRET</system-reminder>"
                )]))
                .content("hi again")
                .build(),
        )
        .send()
        .await?;
    assert_eq!(resp2.status(), 200);
    let received = mock_server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    let last = received
        .last()
        .ok_or_else(|| anyhow::anyhow!("expected at least one upstream request"))?;
    assert!(
        String::from_utf8_lossy(&last.body).contains("SECRET"),
        "live traffic must be untouched by a dry-run"
    );
    Ok(())
}

#[tokio::test]
async fn dry_run_with_draft_rules_previews_without_persisting() -> anyhow::Result<()> {
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

    let (proxy_url, inspect_url) = common::start_proxy(&mock_server.uri()).await?;
    let client = reqwest::Client::new();

    let request_id = capture_request_with_reminder(&client, &proxy_url, &inspect_url).await?;
    assert_dry_run_preview_drops_reminder(&client, &inspect_url, request_id).await?;
    assert_no_rules_persisted(&client, &inspect_url).await?;
    assert_second_request_is_unfiltered(&client, &proxy_url, &mock_server).await?;

    Ok(())
}

#[tokio::test]
async fn dry_run_invalid_draft_rule_returns_400() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    let (_proxy_url, inspect_url) = common::start_proxy(&mock_server.uri()).await?;

    let dry_resp = reqwest::Client::new()
        .post(format!("{inspect_url}/api/filter/rules/dry-run"))
        .json(&json!({
            "request_id": 1,
            "rules": [
                {"kind": "xml_tag", "value": "x", "action": "drop", "text": "must not be here"}
            ]
        }))
        .send()
        .await?;
    assert_eq!(dry_resp.status(), 400);
    let err: Value = dry_resp.json().await?;
    let msg = err["error"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("error body must carry a message"))?;
    assert!(
        msg.contains("must not have a `text`"),
        "validator message must surface verbatim, got: {msg}"
    );
    Ok(())
}

#[tokio::test]
async fn dry_run_firing_rule_ids_report_real_ids_out_of_order() -> anyhow::Result<()> {
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

    let (proxy_url, inspect_url) = common::start_proxy(&mock_server.uri()).await?;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", session_id())
        .json(
            &RequestBody::default()
                .max_tokens(100)
                .system(json!([
                    text_block("<foo>drop foo</foo>"),
                    text_block("<bar>drop bar</bar>"),
                    text_block("<baz>drop baz</baz>"),
                ]))
                .messages(json!([user_message(json!([text_block("hi")]))]))
                .build(),
        )
        .send()
        .await?;
    assert_eq!(resp.status(), 200);

    let requests: Value = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?
        .json()
        .await?;
    let request_id = requests[0]["id"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("request id must be a number"))?;

    let dry_resp = client
        .post(format!("{inspect_url}/api/filter/rules/dry-run"))
        .json(&json!({
            "request_id": request_id,
            "rules": [
                {"id": "500", "kind": "xml_tag", "value": "foo", "action": "drop"},
                {"id": "77", "kind": "xml_tag", "value": "bar", "action": "drop"},
                {"kind": "xml_tag", "value": "baz", "action": "drop"}
            ]
        }))
        .send()
        .await?;
    assert_eq!(dry_resp.status(), 200);
    let preview: Value = dry_resp.json().await?;

    assert_eq!(preview["changed"], true, "all three drop rules must fire");
    assert_eq!(
        preview["firing_rule_ids"],
        json!([500, 77, -3]),
        "must echo each submitted rule's real `id` (out of positional order), and a \
         negated position for the one rule with no `id`, never positions 1..=N: {preview}"
    );
    Ok(())
}

#[tokio::test]
async fn dry_run_unknown_action_names_draft_position() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    let (_proxy_url, inspect_url) = common::start_proxy(&mock_server.uri()).await?;

    let dry_resp = reqwest::Client::new()
        .post(format!("{inspect_url}/api/filter/rules/dry-run"))
        .json(&json!({
            "request_id": 1,
            "rules": [
                {"kind": "xml_tag", "value": "ok", "action": "drop"},
                {"kind": "xml_tag", "value": "bad", "action": "obliterate"}
            ]
        }))
        .send()
        .await?;
    assert_eq!(dry_resp.status(), 400);
    let err: Value = dry_resp.json().await?;
    let msg = err["error"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("error body must carry a message"))?;
    assert!(
        msg.contains("draft rule 2") && msg.contains("unknown action"),
        "error must name the offending draft position, got: {msg}"
    );
    Ok(())
}
