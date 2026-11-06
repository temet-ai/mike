use crate::common;
use crate::filter_proxy;

use crate::anthropic_fixtures;

use super::support::start_proxy_dropping_system_reminder;
use anthropic_fixtures::{RequestBody, text_block, user_message};
use filter_proxy::{claude_response_json, session_id};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn filter_empty_rule_list_sends_byte_identical_body() -> anyhow::Result<()> {
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

    let (proxy_url, _inspect_url) = common::start_proxy(&mock_server.uri()).await?;

    let client_body = RequestBody::default()
        .max_tokens(100)
        .system(json!([text_block("You are Claude Code.")]))
        .messages(json!([user_message(json!([text_block(
            "<system-reminder>unchanged</system-reminder>"
        )]))]))
        .build();
    let client_body_bytes = serde_json::to_vec(&client_body)?;

    let resp = reqwest::Client::new()
        .post(format!("{proxy_url}/v1/messages"))
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
    assert_eq!(received.len(), 1);

    assert_eq!(
        received[0].body, client_body_bytes,
        "empty filter list: upstream body must be byte-identical to client body"
    );

    Ok(())
}

#[tokio::test]
async fn response_bytes_are_unchanged_when_filter_fires_on_request() -> anyhow::Result<()> {
    let fixed_response = claude_response_json();
    let fixed_response_bytes = serde_json::to_vec(&fixed_response)?;

    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(fixed_response_bytes.clone())
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
                .messages(json!([user_message(json!([text_block(
                    "<system-reminder>strip me</system-reminder>"
                )]))]))
                .build(),
        )
        .send()
        .await?;

    assert_eq!(resp.status(), 200);
    let resp_bytes = resp.bytes().await?;

    assert_eq!(
        resp_bytes.as_ref(),
        fixed_response_bytes.as_slice(),
        "response bytes must be identical to what wiremock sent"
    );

    proxy.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn filter_handles_malformed_json_by_forwarding_untouched() -> anyhow::Result<()> {
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

    let raw_body = b"not valid json at all {{{".to_vec();

    let resp = reqwest::Client::new()
        .post(format!("{}/v1/messages", proxy.proxy_url))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", session_id())
        .body(raw_body.clone())
        .send()
        .await?;

    assert_eq!(resp.status(), 200, "non-JSON body must still return 200");

    let received = mock_server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    assert_eq!(received.len(), 1);
    assert_eq!(
        received[0].body, raw_body,
        "malformed JSON must be forwarded byte-identical"
    );

    proxy.cleanup().await?;
    Ok(())
}
