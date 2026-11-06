use crate::db_helpers;
use crate::sse;

use serde_json::{Value, json};
use sse::sse_body;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const DECLARED_TOOL: &str = "mike_session_roster";

const FOLLOWUP_MARKER: &str = "ANSWERED-AFTER-THE-PROXY-RAN-THE-TOOL";

const SESSION_HEADER: &str = "x-claude-code-session-id";

const CONTROL_SESSION: &str = "b1ffcd88-1d2c-4fa9-9c7e-7cc0ce491b22";

const FEATURE_SESSION: &str = "c2a0de99-2e3d-4b0a-8d8f-8dd1df5a2c33";

const TEXT_FIRST_SESSION: &str = "d3b1ef00-3f4e-4c1b-9e9f-9ee2e06b3d44";

const OPENING_TEXT: &str = "Let me check who else is here.";

fn text_then_tool_call_sse() -> String {
    let text = json!({
        "type": "content_block_delta",
        "index": 0,
        "delta": {"type": "text_delta", "text": OPENING_TEXT}
    });
    let start = json!({
        "type": "content_block_start",
        "index": 1,
        "content_block": {"type": "tool_use", "id": "toolu_204spike", "name": DECLARED_TOOL}
    });
    sse_body(&[
        (
            "message_start",
            r#"{"type":"message_start","message":{"id":"msg_204_textfirst"}}"#,
        ),
        (
            "content_block_start",
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
        ),
        ("content_block_delta", &text.to_string()),
        (
            "content_block_stop",
            r#"{"type":"content_block_stop","index":0}"#,
        ),
        ("content_block_start", &start.to_string()),
        (
            "content_block_delta",
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{}"}}"#,
        ),
        (
            "content_block_stop",
            r#"{"type":"content_block_stop","index":1}"#,
        ),
        (
            "message_delta",
            r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"}}"#,
        ),
        ("message_stop", r#"{"type":"message_stop"}"#),
    ])
}

fn client_frames(client_saw: &str) -> Vec<(String, Value)> {
    client_saw
        .split("\n\n")
        .filter_map(|frame| {
            let event = frame.lines().find_map(|l| l.strip_prefix("event: "))?;
            let data = frame.lines().find_map(|l| l.strip_prefix("data: "))?;
            Some((event.to_owned(), serde_json::from_str(data).ok()?))
        })
        .collect()
}

fn tool_call_sse() -> String {
    let start = json!({
        "type": "content_block_start",
        "index": 0,
        "content_block": {"type": "tool_use", "id": "toolu_204spike", "name": DECLARED_TOOL}
    });
    sse_body(&[
        (
            "message_start",
            r#"{"type":"message_start","message":{"id":"msg_204_toolcall"}}"#,
        ),
        ("content_block_start", &start.to_string()),
        (
            "content_block_delta",
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{}"}}"#,
        ),
        (
            "content_block_stop",
            r#"{"type":"content_block_stop","index":0}"#,
        ),
        (
            "message_delta",
            r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"}}"#,
        ),
        ("message_stop", r#"{"type":"message_stop"}"#),
    ])
}

fn followup_sse() -> String {
    let delta = json!({
        "type": "content_block_delta",
        "index": 0,
        "delta": {"type": "text_delta", "text": FOLLOWUP_MARKER}
    });
    sse_body(&[
        (
            "message_start",
            r#"{"type":"message_start","message":{"id":"msg_204_followup"}}"#,
        ),
        (
            "content_block_start",
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
        ),
        ("content_block_delta", &delta.to_string()),
        (
            "content_block_stop",
            r#"{"type":"content_block_stop","index":0}"#,
        ),
        (
            "message_delta",
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"}}"#,
        ),
        ("message_stop", r#"{"type":"message_stop"}"#),
    ])
}

fn sse_response(body: String) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body.into_bytes(), "text/event-stream")
}

async fn mount_upstream(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header(SESSION_HEADER, CONTROL_SESSION))
        .respond_with(sse_response(tool_call_sse()))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header(SESSION_HEADER, FEATURE_SESSION))
        .respond_with(sse_response(tool_call_sse()))
        .up_to_n_times(1)
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header(SESSION_HEADER, FEATURE_SESSION))
        .respond_with(sse_response(followup_sse()))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header(SESSION_HEADER, TEXT_FIRST_SESSION))
        .respond_with(sse_response(text_then_tool_call_sse()))
        .up_to_n_times(1)
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header(SESSION_HEADER, TEXT_FIRST_SESSION))
        .respond_with(sse_response(followup_sse()))
        .mount(server)
        .await;
}

async fn set_proxy_tools(inspect_url: &str, enabled: bool) -> anyhow::Result<()> {
    let resp = reqwest::Client::new()
        .patch(format!("{inspect_url}/api/settings/features/proxy_tools"))
        .json(&json!({"enabled": enabled}))
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "setting proxy_tools to {enabled} returned {}",
        resp.status()
    );
    Ok(())
}

async fn drive_request(proxy_url: &str, session_id: &str) -> anyhow::Result<String> {
    let body = json!({
        "model": "claude-opus-4-5",
        "max_tokens": 1024,
        "stream": true,
        "messages": [{"role": "user", "content": "who else is working in this project?"}],
        "system": "You are a helpful assistant.",
        "tools": [{
            "name": "Read",
            "description": "Read a file",
            "input_schema": {"type": "object", "properties": {}}
        }],
    });
    let resp = reqwest::Client::new()
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header(SESSION_HEADER, session_id)
        .json(&body)
        .send()
        .await?;
    anyhow::ensure!(resp.status() == 200, "expected 200, got {}", resp.status());
    Ok(String::from_utf8_lossy(&resp.bytes().await?).to_string())
}

fn assert_text_first_reply_is_one_stitched_message(client_saw: &str) -> anyhow::Result<()> {
    assert!(
        client_saw.contains(OPENING_TEXT),
        "text before the call was streamed as it arrived; got:\n{client_saw}"
    );
    assert!(
        !client_saw.contains("toolu_204spike"),
        "client must never see the proxy-declared tool_use; got:\n{client_saw}"
    );
    let frames = client_frames(client_saw);
    let kinds: Vec<&str> = frames.iter().map(|(event, _)| event.as_str()).collect();
    assert_eq!(
        kinds.iter().filter(|k| **k == "message_start").count(),
        1,
        "the answer continues the message the client already has: {kinds:?}"
    );
    assert!(
        client_saw.contains("msg_204_textfirst") && !client_saw.contains("msg_204_followup"),
        "the one message keeps the first reply's id; got:\n{client_saw}"
    );
    let answer = frames
        .iter()
        .find(|(_, data)| data["delta"]["text"] == FOLLOWUP_MARKER)
        .ok_or_else(|| anyhow::anyhow!("the follow-up answer must be streamed: {kinds:?}"))?;
    assert_eq!(
        answer.1["index"], 1,
        "the answer's block follows the one text block the client already has"
    );
    assert_eq!(
        kinds
            .iter()
            .filter(|k| **k == "message_delta" || **k == "message_stop")
            .count(),
        2,
        "the message ends once, with the follow-up's ending: {kinds:?}"
    );
    Ok(())
}

fn tool_names(request: &Value) -> anyhow::Result<Vec<&str>> {
    Ok(request["tools"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("request has no tools array"))?
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect())
}

#[tokio::test]
async fn proxy_declares_a_tool_answers_the_call_and_resolves_the_turn() -> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    mount_upstream(&upstream).await;

    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping");
        return Ok(());
    };

    let control_saw = drive_request(&proxy.proxy_url, CONTROL_SESSION).await?;
    assert!(
        control_saw.contains("toolu_204spike"),
        "with the toggle off the tool_use must stream through untouched; got:\n{control_saw}"
    );
    let received = upstream.received_requests().await.unwrap_or_default();
    assert_eq!(
        received.len(),
        1,
        "with the toggle off there is no second upstream request"
    );
    let control_request: Value = serde_json::from_slice(&received[0].body)?;
    assert_eq!(
        tool_names(&control_request)?,
        vec!["Read"],
        "with the toggle off the client's tools array must be forwarded unchanged"
    );

    set_proxy_tools(&proxy.inspect_url, true).await?;
    let client_saw = drive_request(&proxy.proxy_url, FEATURE_SESSION).await;
    let text_first_saw = drive_request(&proxy.proxy_url, TEXT_FIRST_SESSION).await;
    set_proxy_tools(&proxy.inspect_url, false).await?;
    let client_saw = client_saw?;
    let text_first_saw = text_first_saw?;

    assert_text_first_reply_is_one_stitched_message(&text_first_saw)?;

    assert!(
        client_saw.contains(FOLLOWUP_MARKER),
        "client must receive the follow-up answer; got:\n{client_saw}"
    );
    assert!(
        !client_saw.contains("toolu_204spike"),
        "client must never see the proxy-declared tool_use; got:\n{client_saw}"
    );

    let received = upstream.received_requests().await.unwrap_or_default();
    assert_eq!(
        received.len(),
        5,
        "the proxy must send a second upstream request carrying the tool result (one control request plus two two-leg loops)"
    );

    let first: Value = serde_json::from_slice(&received[1].body)?;
    assert_eq!(
        tool_names(&first)?,
        vec!["Read", DECLARED_TOOL],
        "the client's own tools must come first, with the declared tool appended"
    );

    let second: Value = serde_json::from_slice(&received[2].body)?;
    let messages = second["messages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("second request has no messages array"))?;
    assert!(
        messages.len() >= 3,
        "follow-up must append an assistant turn and a user turn; got {} messages",
        messages.len()
    );
    let assistant = &messages[messages.len() - 2];
    let user = &messages[messages.len() - 1];
    assert_eq!(assistant["role"], "assistant");
    assert_eq!(assistant["content"][0]["type"], "tool_use");
    assert_eq!(assistant["content"][0]["name"], DECLARED_TOOL);
    assert_eq!(user["role"], "user");
    assert_eq!(user["content"][0]["type"], "tool_result");
    assert_eq!(
        user["content"][0]["tool_use_id"], assistant["content"][0]["id"],
        "the tool_result must echo the tool_use id or Anthropic 400s the request"
    );
    let result_text = user["content"][0]["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("tool_result carries no text"))?;
    assert!(
        !result_text.trim().is_empty(),
        "the proxy must answer the call with something, not an empty result"
    );

    assert!(
        tool_names(&second)?.contains(&DECLARED_TOOL),
        "the follow-up must keep the declared tool; got {:?}",
        tool_names(&second)?
    );

    proxy.shutdown.cancel();
    proxy.test_db.cleanup().await?;
    Ok(())
}
