use super::*;
use serde_json::{Value, json};
use wiremock::matchers::{body_partial_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const CHILD: &str = "00000000-0000-4000-8000-000000000001";
const PARENT: &str = "00000000-0000-4000-8000-000000000002";
const RECIPIENT: &str = "00000000-0000-4000-8000-000000000003";

fn request(
    name: &str,
    arguments: Value,
    thread: Option<&str>,
) -> anyhow::Result<CallToolRequestParams> {
    let mut value = json!({"name": name, "arguments": arguments});
    if let Some(thread) = thread {
        value["_meta"] = json!({"threadId": thread});
    }
    Ok(serde_json::from_value(value)?)
}

async fn bind(mock: &MockServer, thread: &str) {
    Mock::given(method("GET"))
        .and(path("/api/codex/session"))
        .and(query_param("thread_id", thread))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"session_id": CHILD})))
        .mount(mock)
        .await;
}

async fn accept_mail(mock: &MockServer, sender: &str) {
    Mock::given(method("POST"))
        .and(path(format!("/api/sessions/{RECIPIENT}/message")))
        .and(body_partial_json(
            json!({"message": "hello", "from_session_id": sender}),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"from_session_uuid": sender})),
        )
        .expect(1)
        .mount(mock)
        .await;
}

fn result_value(result: CallToolResponse) -> anyhow::Result<Value> {
    let CallToolResponse::Complete(result) = result else {
        anyhow::bail!("tool call did not complete");
    };
    result
        .structured_content
        .ok_or_else(|| anyhow::anyhow!("structured result missing"))
}

#[tokio::test]
async fn mail_codex_request_identity_overrides_inherited_process_identity() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    bind(&mock, "native-child").await;
    accept_mail(&mock, CHILD).await;
    let mut server = McpServer::new(
        InspectClient::new(mock.uri()),
        Profile::Core,
        Some(PARENT.into()),
    );
    server.codex_thread_id = Some("native-parent".into());
    let result = server
        .dispatch_request(request(
            "send_session_message",
            json!({
                "to": RECIPIENT, "message": "hello"
            }),
            Some("native-child"),
        )?)
        .await?;
    let value = result_value(result)?;
    assert_eq!(value["message"]["from_session_uuid"], CHILD);
    assert!(value["message"].get("from_unverified").is_none());
    Ok(())
}

#[tokio::test]
async fn mail_codex_environment_identity_resolves_without_claude_capture() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    bind(&mock, "native-current").await;
    accept_mail(&mock, CHILD).await;
    let mut server = McpServer::new(InspectClient::new(mock.uri()), Profile::Core, None);
    server.codex_thread_id = Some("native-current".into());
    server
        .dispatch_request(request(
            "send_session_message",
            json!({
                "to": RECIPIENT, "message": "hello"
            }),
            None,
        )?)
        .await?;
    Ok(())
}

#[tokio::test]
async fn mail_codex_explicit_parent_sender_is_refused_before_sending() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    bind(&mock, "native-child").await;
    let server = McpServer::new(
        InspectClient::new(mock.uri()),
        Profile::Core,
        Some(PARENT.into()),
    );
    let error = server
        .dispatch_request(request(
            "send_session_message",
            json!({
                "to": RECIPIENT, "message": "hello", "from": PARENT
            }),
            Some("native-child"),
        )?)
        .await
        .err()
        .ok_or_else(|| anyhow::anyhow!("parent sender accepted"))?;
    assert!(error.message.contains(CHILD));
    assert!(error.message.contains(PARENT));
    let requests = mock
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("requests unavailable"))?;
    assert!(requests.iter().all(|request| request.method == "GET"));
    Ok(())
}

#[tokio::test]
async fn mail_process_identity_is_unverified_fallback_after_capture_lookup() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    accept_mail(&mock, CHILD).await;
    Mock::given(method("GET"))
        .and(path(format!("/api/sessions/{CHILD}/head")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "newest_request_id": 1, "prompt_kind_counts": {"operator": 1}
        })))
        .expect(1)
        .mount(&mock)
        .await;
    let server = McpServer::new(
        InspectClient::new(mock.uri()),
        Profile::Core,
        Some(CHILD.into()),
    );
    let result = server
        .dispatch_request(request(
            "send_session_message",
            json!({
                "to": RECIPIENT, "message": "hello"
            }),
            None,
        )?)
        .await?;
    assert_eq!(result_value(result)?["message"]["from_unverified"], true);
    Ok(())
}

#[tokio::test]
async fn mail_captured_claude_sender_wins_over_stale_process_identity() -> anyhow::Result<()> {
    for explicit in [None, Some(CHILD)] {
        let mock = MockServer::start().await;
        accept_mail(&mock, CHILD).await;
        Mock::given(method("GET"))
            .and(path("/api/body/find"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "matches": [{"request_id": 7, "session_id": CHILD}]
            })))
            .mount(&mock)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/requests/7/tool-calls"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "calls": [{"name": "mcp__mike__send_session_message", "input": {"message": "hello"}}]
            })))
            .mount(&mock).await;
        let server = McpServer::new(
            InspectClient::new(mock.uri()),
            Profile::Core,
            Some(PARENT.into()),
        );
        let mut arguments = json!({"to": RECIPIENT, "message": "hello"});
        if let Some(explicit) = explicit {
            arguments["from"] = json!(explicit);
        }
        let result = server
            .dispatch_request(request("send_session_message", arguments, None)?)
            .await?;
        let value = result_value(result)?;
        assert_eq!(value["message"]["from_session_uuid"], CHILD);
        assert!(value["message"].get("from_unverified").is_none());
    }
    Ok(())
}

#[tokio::test]
async fn mail_unbound_child_cannot_send_as_inherited_or_explicit_parent() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    let server = McpServer::new(
        InspectClient::new(mock.uri()),
        Profile::Core,
        Some(PARENT.into()),
    );
    let error = server
        .dispatch_request(request(
            "send_session_message",
            json!({
                "to": RECIPIENT, "message": "hello", "from": PARENT
            }),
            Some("unbound-child"),
        )?)
        .await
        .err()
        .ok_or_else(|| anyhow::anyhow!("unbound child sent mail"))?;
    assert!(error.message.contains("unbound-child"));
    let requests = mock
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("requests unavailable"))?;
    assert!(requests.iter().all(|request| request.method == "GET"));
    Ok(())
}

#[tokio::test]
async fn mail_codex_inbox_and_ack_default_to_resolved_child() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    bind(&mock, "native-child").await;
    Mock::given(method("GET"))
        .and(path(format!("/api/sessions/{CHILD}/messages")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"inbox": [], "outbox": []})))
        .expect(1)
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/api/sessions/{CHILD}/messages/message/ack")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"acknowledged": true})))
        .expect(1)
        .mount(&mock)
        .await;
    let server = McpServer::new(
        InspectClient::new(mock.uri()),
        Profile::Core,
        Some(PARENT.into()),
    );
    server
        .dispatch_request(request(
            "get_session_messages",
            json!({}),
            Some("native-child"),
        )?)
        .await?;
    server
        .dispatch_request(request(
            "ack_session_message",
            json!({"message_uuid": "message"}),
            Some("native-child"),
        )?)
        .await?;
    Ok(())
}

#[tokio::test]
async fn mail_caller_identity_in_tool_arguments_is_refused() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    let server = McpServer::new(InspectClient::new(mock.uri()), Profile::Core, None);
    let error = server
        .dispatch_request(request(
            "send_session_message",
            json!({
                "to": RECIPIENT, "message": "hello", "caller_session": CHILD
            }),
            None,
        )?)
        .await
        .err()
        .ok_or_else(|| anyhow::anyhow!("tool argument impersonated caller"))?;
    assert!(error.message.contains("unknown argument"));
    assert!(
        mock.received_requests()
            .await
            .ok_or_else(|| anyhow::anyhow!("requests unavailable"))?
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn mail_malformed_native_metadata_does_not_use_process_parent() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    let server = McpServer::new(
        InspectClient::new(mock.uri()),
        Profile::Core,
        Some(PARENT.into()),
    );
    let mut malformed = request(
        "send_session_message",
        json!({
            "to": RECIPIENT, "message": "hello"
        }),
        None,
    )?;
    malformed.meta = Some(serde_json::from_value(
        json!({"x-codex-turn-metadata": {}}),
    )?);
    assert!(server.dispatch_request(malformed).await.is_err());
    assert!(
        mock.received_requests()
            .await
            .ok_or_else(|| anyhow::anyhow!("requests unavailable"))?
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn inspection_unbound_codex_thread_can_still_read_an_explicit_session() -> anyhow::Result<()>
{
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/sessions/{PARENT}/head")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"session_id": PARENT})))
        .expect(1)
        .mount(&mock)
        .await;
    let server = McpServer::new(InspectClient::new(mock.uri()), Profile::Core, None);
    server
        .dispatch_request(request(
            "get_session_head",
            json!({"session_id": PARENT}),
            Some("unbound"),
        )?)
        .await?;
    Ok(())
}
