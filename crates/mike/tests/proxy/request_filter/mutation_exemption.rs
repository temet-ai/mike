use crate::anthropic_fixtures;
use crate::filter_proxy;

use super::support::start_proxy_dropping_system_reminder;
use anthropic_fixtures::{RequestBody, text_block, user_message};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn body_with_system_reminder() -> anyhow::Result<Vec<u8>> {
    let body = RequestBody::default()
        .max_tokens(100)
        .messages(json!([user_message(json!([text_block(
            "<system-reminder>strip me</system-reminder>"
        )]))]))
        .build();
    Ok(serde_json::to_vec(&body)?)
}

#[tokio::test]
async fn mutation_exempt_header_bypasses_active_filter_rule() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(filter_proxy::claude_response_json())
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let Some(proxy) = start_proxy_dropping_system_reminder(&mock_server.uri()).await? else {
        return Ok(());
    };

    let client_body_bytes = body_with_system_reminder()?;

    let resp = reqwest::Client::new()
        .post(format!("{}/v1/messages", proxy.proxy_url))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", filter_proxy::session_id())
        .header("x-mike-client", "prompt-assistant")
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
        "x-mike-client: prompt-assistant must bypass the operator's filter rules and \
         forward the body byte-identical, even with an active drop rule"
    );

    proxy.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn same_rule_still_fires_without_the_exemption_header() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(filter_proxy::claude_response_json())
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let Some(proxy) = start_proxy_dropping_system_reminder(&mock_server.uri()).await? else {
        return Ok(());
    };

    let client_body_bytes = body_with_system_reminder()?;

    let resp = reqwest::Client::new()
        .post(format!("{}/v1/messages", proxy.proxy_url))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", filter_proxy::session_id())
        .body(client_body_bytes.clone())
        .send()
        .await?;

    assert_eq!(resp.status(), 200);

    let received = mock_server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    assert_eq!(received.len(), 1);
    assert_ne!(
        received[0].body, client_body_bytes,
        "without the exemption header, the drop rule must still fire on ordinary traffic"
    );

    proxy.cleanup().await?;
    Ok(())
}
