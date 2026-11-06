use crate::anthropic_fixtures;
use crate::filter_proxy;

use anthropic_fixtures::{RequestBody, text_block};
use filter_proxy::{StartedFilterProxy, claude_response_json, session_id, start_proxy_with_rules};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn mount_ok_mock() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(claude_response_json())
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn xmltag_self_closing_does_not_match() -> anyhow::Result<()> {
    let server = mount_ok_mock().await;
    let Some(proxy) = start_proxy_with_rules(
        &server.uri(),
        &[json!({"kind": "xml_tag", "value": "foo", "action": "drop"})],
    )
    .await?
    else {
        return Ok(());
    };

    let client_body = RequestBody::default()
        .system(json!([text_block("<foo/>")]))
        .content("ping")
        .max_tokens(100)
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

    let received = server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    let body: Value = serde_json::from_slice(&received[0].body)?;
    let arr = body["system"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("system must be array"))?;
    assert_eq!(arr.len(), 1, "self-closing block must remain");
    assert_eq!(arr[0]["text"], "<foo/>");

    proxy.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn string_shaped_content_is_untouched() -> anyhow::Result<()> {
    let server = mount_ok_mock().await;
    let Some(proxy) = start_proxy_with_rules(
        &server.uri(),
        &[json!({"kind": "xml_tag", "value": "system-reminder", "action": "drop"})],
    )
    .await?
    else {
        return Ok(());
    };

    let client_body = RequestBody::default()
        .content("<system-reminder>do not touch this</system-reminder>")
        .max_tokens(100)
        .build();
    let bytes = serde_json::to_vec(&client_body)?;
    let resp = reqwest::Client::new()
        .post(format!("{}/v1/messages", proxy.proxy_url))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", session_id())
        .body(bytes.clone())
        .send()
        .await?;
    assert_eq!(resp.status(), 200);

    let received = server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    assert_eq!(
        received[0].body, bytes,
        "string-shaped content must be untouched"
    );

    proxy.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn disabled_rule_is_skipped() -> anyhow::Result<()> {
    let server = mount_ok_mock().await;
    let Some(proxy) = start_proxy_with_rules(
        &server.uri(),
        &[json!({
            "kind": "xml_tag",
            "value": "system-reminder",
            "action": "drop",
            "enabled": false,
        })],
    )
    .await?
    else {
        return Ok(());
    };

    let client_body = RequestBody::default()
        .system(json!([text_block(
            "<system-reminder>still here</system-reminder>"
        )]))
        .content("ping")
        .max_tokens(100)
        .build();
    let bytes = serde_json::to_vec(&client_body)?;
    let resp = reqwest::Client::new()
        .post(format!("{}/v1/messages", proxy.proxy_url))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", session_id())
        .body(bytes.clone())
        .send()
        .await?;
    assert_eq!(resp.status(), 200);

    let received = server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    assert_eq!(received[0].body, bytes, "disabled rule must not fire");

    proxy.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn utf8_block_text_matches_xml_tag() -> anyhow::Result<()> {
    let server = mount_ok_mock().await;
    let Some(proxy) = start_proxy_with_rules(
        &server.uri(),
        &[json!({"kind": "xml_tag", "value": "badge", "action": "drop"})],
    )
    .await?
    else {
        return Ok(());
    };

    let body_text = "<badge>\u{1F389}\u{0301}a\u{0308}</badge>";
    let client_body = RequestBody::default()
        .system(json!([text_block(body_text)]))
        .content("ping")
        .max_tokens(100)
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

    let received = server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    let body: Value = serde_json::from_slice(&received[0].body)?;
    let arr = body["system"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("system must be array"))?;
    assert!(
        arr.is_empty(),
        "emoji-bearing block must still match the XmlTag rule"
    );

    proxy.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn xmltag_is_case_sensitive() -> anyhow::Result<()> {
    let server = mount_ok_mock().await;
    let Some(proxy) = start_proxy_with_rules(
        &server.uri(),
        &[json!({"kind": "xml_tag", "value": "system-reminder", "action": "drop"})],
    )
    .await?
    else {
        return Ok(());
    };

    let client_body = RequestBody::default()
        .system(json!([text_block(
            "<System-Reminder>not matched</System-Reminder>"
        )]))
        .content("ping")
        .max_tokens(100)
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

    let received = server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    let body: Value = serde_json::from_slice(&received[0].body)?;
    let arr = body["system"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("system must be array"))?;
    assert_eq!(arr.len(), 1, "case-mismatched XmlTag must NOT match");

    proxy.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn text_substring_is_case_insensitive() -> anyhow::Result<()> {
    let server = mount_ok_mock().await;
    let Some(proxy) = start_proxy_with_rules(
        &server.uri(),
        &[json!({
            "kind": "text_substring",
            "value": "THE FOLLOWING SKILLS ARE AVAILABLE IN THIS SESSION",
            "action": "replace",
            "text": "Skills present; invoke by name.",
        })],
    )
    .await?
    else {
        return Ok(());
    };

    let client_body = RequestBody::default()
        .system(json!([text_block(
            "the following skills are available in this session"
        )]))
        .content("ping")
        .max_tokens(100)
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

    let received = server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    let body: Value = serde_json::from_slice(&received[0].body)?;
    let block = &body["system"][0];
    assert_eq!(block["text"], "Skills present; invoke by name.");

    proxy.cleanup().await?;
    Ok(())
}

async fn fire_request_and_observe(
    proxy: &StartedFilterProxy,
    server: &MockServer,
    reminder_text: &str,
) -> anyhow::Result<bool> {
    let client_body = RequestBody::default()
        .system(json!([text_block(reminder_text)]))
        .content("ping")
        .max_tokens(100)
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
    let received = server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("no upstream requests"))?;
    let last = received
        .last()
        .ok_or_else(|| anyhow::anyhow!("upstream request list empty"))?;
    let body: Value = serde_json::from_slice(&last.body)?;
    let system_arr = body["system"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("system must be an array"))?;
    Ok(system_arr.is_empty())
}

#[tokio::test]
async fn rule_fires_after_toggling_enabled_back_to_true() -> anyhow::Result<()> {
    let server = mount_ok_mock().await;
    let Some(proxy) = start_proxy_with_rules(
        &server.uri(),
        &[json!({
            "kind": "xml_tag",
            "value": "system-reminder",
            "action": "drop",
            "enabled": false,
        })],
    )
    .await?
    else {
        return Ok(());
    };
    let matched_when_disabled = fire_request_and_observe(
        &proxy,
        &server,
        "<system-reminder>toggle-probe</system-reminder>",
    )
    .await?;
    assert!(
        !matched_when_disabled,
        "disabled rule must not fire on block"
    );

    let client = reqwest::Client::new();
    let rules: Vec<Value> = client
        .get(format!("{}/api/filter/rules", proxy.inspect_url))
        .send()
        .await?
        .json()
        .await?;
    let rule_id = rules
        .first()
        .and_then(|r| r["id"].as_str())
        .ok_or_else(|| anyhow::anyhow!("seeded rule missing from list: {rules:?}"))?
        .to_string();
    let patch = client
        .patch(format!("{}/api/filter/rules/{rule_id}", proxy.inspect_url))
        .json(&json!({"enabled": true}))
        .send()
        .await?;
    anyhow::ensure!(
        patch.status() == 200,
        "PATCH enabled=true must succeed, got {}",
        patch.status()
    );

    let matched_when_enabled = fire_request_and_observe(
        &proxy,
        &server,
        "<system-reminder>toggle-probe-2</system-reminder>",
    )
    .await?;
    assert!(
        matched_when_enabled,
        "enabled rule MUST fire on matching block"
    );

    proxy.cleanup().await?;
    Ok(())
}
