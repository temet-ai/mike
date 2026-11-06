use crate::filter_proxy;

use crate::anthropic_fixtures;

use anthropic_fixtures::{
    RequestBody, assistant_message, cached_text_block, text_block, user_message,
};
use filter_proxy::{claude_response_json, session_id, start_proxy_with_rules};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn filter_drops_assistant_turn_content_block() -> anyhow::Result<()> {
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
        &[json!({"kind": "xml_tag", "value": "tool-debug", "action": "drop"})],
    )
    .await?
    else {
        return Ok(());
    };

    let client_body = RequestBody::default()
        .max_tokens(100)
        .messages(json!([
            user_message(json!([text_block("ok")])),
            assistant_message(json!([
                text_block("<tool-debug>noisy</tool-debug>"),
                text_block("keeper"),
            ])),
            user_message(json!([text_block("ack")])),
        ]))
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
    let body: Value = serde_json::from_slice(&received[0].body)?;

    let assistant_content = body["messages"][1]["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("assistant content must be array"))?;
    assert_eq!(
        assistant_content.len(),
        1,
        "filter must drop the <tool-debug> block from the assistant turn; got: {assistant_content:?}"
    );
    assert_eq!(assistant_content[0]["text"], "keeper");

    assert_eq!(body["messages"][0]["content"][0]["text"], "ok");
    assert_eq!(body["messages"][2]["content"][0]["text"], "ack");

    proxy.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn filter_dropped_block_preserves_other_assistant_blocks() -> anyhow::Result<()> {
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
        &[json!({"kind": "xml_tag", "value": "tool-debug", "action": "drop"})],
    )
    .await?
    else {
        return Ok(());
    };

    let client_body = RequestBody::default()
        .max_tokens(100)
        .messages(json!([
            user_message(json!([text_block("hi")])),
            assistant_message(json!([
                cached_text_block("first survivor"),
                text_block("<tool-debug>drop-me</tool-debug>"),
                text_block("third survivor"),
            ])),
        ]))
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
    let body: Value = serde_json::from_slice(&received[0].body)?;
    let arr = body["messages"][1]["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("assistant content must be array"))?;

    assert_eq!(
        arr.len(),
        2,
        "middle block dropped; first + third survive; got: {arr:?}"
    );
    assert_eq!(arr[0]["text"], "first survivor");
    assert_eq!(
        arr[0]["cache_control"]["type"], "ephemeral",
        "cache_control on surviving blocks must be preserved"
    );
    assert_eq!(arr[1]["text"], "third survivor");
    let serialized = serde_json::to_string(&body)?;
    assert!(
        !serialized.contains("drop-me"),
        "dropped block text leaked into upstream body: {serialized}"
    );

    proxy.cleanup().await?;
    Ok(())
}
