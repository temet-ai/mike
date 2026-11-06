use tokio::io::AsyncWriteExt;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::common::{
    call_tool_request, read_next_message, spawn_mcp_with_inspect, structured_content,
};

const SESSION: &str = "00000000-0000-4000-8000-000000000001";

#[tokio::test]
async fn get_session_dossier_equals_the_rest_body() -> anyhow::Result<()> {
    let rest = serde_json::json!({
        "session_uuid": SESSION,
        "renderer_version": "dossier-r1",
        "view": {"stream": {"total": 1, "items": [{"request_id": 1, "text": "x".repeat(9000)}]}},
        "rendered": {"audience": "steerer", "text": "whole"}
    });
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/sessions/{SESSION}/dossier")))
        .and(query_param("audience", "steerer"))
        .and(query_param("budget_chars", "8000"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&rest))
        .mount(&server)
        .await;
    let (mut child, mut stdin, mut reader) = spawn_mcp_with_inspect(&server.uri()).await?;
    let args =
        serde_json::json!({"session_id": SESSION, "audience": "steerer", "budget_chars": 8000});
    stdin
        .write_all(format!("{}\n", call_tool_request(31, "get_session_dossier", args)).as_bytes())
        .await?;
    let response = read_next_message(&mut reader).await?;
    let structured = structured_content(&response)?;
    assert_eq!(structured.get("dossier"), Some(&rest));
    drop(stdin);
    let _ = child.wait().await;
    Ok(())
}

#[tokio::test]
async fn get_turn_facts_equals_the_rest_body() -> anyhow::Result<()> {
    let rest =
        serde_json::json!({"turn_id": 7, "facts": {"checks_run": [{"command": "cargo test"}]}});
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/operator-turns/7/facts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&rest))
        .mount(&server)
        .await;
    let (mut child, mut stdin, mut reader) = spawn_mcp_with_inspect(&server.uri()).await?;
    stdin
        .write_all(
            format!(
                "{}\n",
                call_tool_request(32, "get_turn_facts", serde_json::json!({"turn_id": 7}))
            )
            .as_bytes(),
        )
        .await?;
    let response = read_next_message(&mut reader).await?;
    assert_eq!(
        structured_content(&response)?.get("turn_facts"),
        Some(&rest)
    );
    drop(stdin);
    let _ = child.wait().await;
    Ok(())
}
