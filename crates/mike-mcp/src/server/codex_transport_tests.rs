use super::*;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use wiremock::matchers::{body_partial_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn codex_mail_transport_metadata_reaches_caller_resolution() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    let sender = "00000000-0000-4000-8000-000000000001";
    let recipient = "00000000-0000-4000-8000-000000000002";
    Mock::given(method("GET"))
        .and(path("/api/codex/session"))
        .and(query_param("thread_id", "native-current"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"session_id": sender})))
        .expect(1)
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/api/sessions/{recipient}/message")))
        .and(body_partial_json(
            json!({"message": "hello", "from_session_id": sender}),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"from_session_uuid": sender})),
        )
        .expect(1)
        .mount(&mock)
        .await;
    let server = McpServer::new(InspectClient::new(mock.uri()), Profile::Core, None);
    let (client_side, server_side) = tokio::io::duplex(1 << 20);
    let transport = LegacyOnly::new(
        IntoTransport::<RoleServer, std::io::Error, TransportAdapterAsyncRW>::into_transport(
            tokio::io::split(server_side),
        ),
        CHANNEL_CAPABLE_PROTOCOL_VERSIONS,
    );
    let serving = tokio::spawn(async move { server.serve(transport).await });
    let (read, mut write) = tokio::io::split(client_side);
    let mut lines = BufReader::new(read).lines();
    for message in [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-11-25", "capabilities": {},
            "clientInfo": {"name": "codex-test", "version": "1"}
        }}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
            "name": "send_session_message", "arguments": {"to": recipient, "message": "hello"},
            "_meta": {"threadId": "native-current"}
        }}),
    ] {
        write.write_all(format!("{message}\n").as_bytes()).await?;
    }
    let answer = loop {
        let line = tokio::time::timeout(std::time::Duration::from_secs(10), lines.next_line())
            .await??
            .ok_or_else(|| anyhow::anyhow!("server closed before answering"))?;
        let reply: Value = serde_json::from_str(&line)?;
        if reply["id"] == 2 {
            break reply;
        }
    };
    assert!(answer.get("error").is_none(), "{answer}");
    assert_eq!(
        answer["result"]["structuredContent"]["message"]["from_session_uuid"],
        sender
    );
    serving.abort();
    Ok(())
}
