use super::*;
use serde_json::{Map, Value, json};
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SESSION: &str = "00000000-0000-4000-8000-000000000001";
const SOURCE: &str = "00000000-0000-4000-8000-000000000002";

fn args(value: Value) -> Option<Map<String, Value>> {
    value.as_object().cloned()
}

fn server(mock: &MockServer, profile: Profile, session: Option<&str>) -> McpServer {
    McpServer::new(
        InspectClient::new(mock.uri()),
        profile,
        session.map(str::to_string),
    )
}

#[tokio::test]
async fn retrospective_writes_and_end_reach_only_the_trusted_session() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("/api/sessions/{SESSION}/notes")))
        .and(body_partial_json(
            json!({"title": "Findings", "kind": "note"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"note_uuid": "new"})))
        .expect(1)
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/api/workspace/register/{SESSION}/end")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ended": true})))
        .expect(1)
        .mount(&mock)
        .await;
    let server = server(&mock, Profile::Retrospective, Some(SESSION));
    server
        .dispatch_tool("write_session_note", args(json!({"title": "Findings"})))
        .await?;
    server
        .dispatch_tool("end_workspace_registration", None)
        .await?;
    mock.verify().await;
    Ok(())
}

#[tokio::test]
async fn retrospective_denied_calls_make_no_http_requests() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    let server = server(&mock, Profile::Retrospective, Some(SESSION));
    for (name, input) in [
        (
            "write_session_note",
            json!({"session_id": SOURCE, "title": "Spoof"}),
        ),
        ("end_workspace_registration", json!({"session_id": SOURCE})),
        (
            "write_session_note",
            json!({"note_uuid": "source-note", "body": "Changed"}),
        ),
        (
            "write_session_note",
            json!({"title": "Carry", "data": {"carry": true}}),
        ),
        (
            "send_session_message",
            json!({"to": SOURCE, "body": "Hello"}),
        ),
        (
            "delete_session_note",
            json!({"session_id": SOURCE, "note_uuid": "source-note"}),
        ),
        ("define_fleet", json!({"definition": {}})),
        ("fleet_check", json!({"fleet": "source"})),
    ] {
        let error = server
            .dispatch_tool(name, args(input))
            .await
            .err()
            .ok_or_else(|| anyhow::anyhow!("restricted call was accepted"))?;
        assert!(error.message.contains("retrospective profile"));
    }
    let requests = mock
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("mock did not record requests"))?;
    assert!(requests.is_empty());
    Ok(())
}

#[tokio::test]
async fn retrospective_without_trusted_identity_cannot_write_even_explicitly() -> anyhow::Result<()>
{
    let mock = MockServer::start().await;
    for session in [None, Some("invalid")] {
        let server = server(&mock, Profile::Retrospective, session);
        for name in ["write_session_note", "end_workspace_registration"] {
            assert!(
                server
                    .dispatch_tool(
                        name,
                        args(json!({"session_id": SESSION, "title": "Finding"}))
                    )
                    .await
                    .is_err()
            );
        }
    }
    let requests = mock
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("mock did not record requests"))?;
    assert!(requests.is_empty());
    Ok(())
}

#[tokio::test]
async fn retrospective_can_read_source_session_notes() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/sessions/{SOURCE}/notes")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&mock)
        .await;
    server(&mock, Profile::Retrospective, Some(SESSION))
        .dispatch_tool("list_session_notes", args(json!({"session_id": SOURCE})))
        .await?;
    mock.verify().await;
    Ok(())
}

#[tokio::test]
async fn ordinary_profiles_preserve_explicit_identity_and_deferred_calls() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path(format!("/api/sessions/{SOURCE}/notes/existing")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"note_uuid": "existing"})))
        .expect(7)
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/api/workspace/register/{SOURCE}/end")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ended": true})))
        .expect(7)
        .mount(&mock)
        .await;
    for profile in [
        Profile::Core,
        Profile::Full,
        Profile::Worker,
        Profile::Coordinator,
        Profile::Acceptance,
        Profile::Observer,
        Profile::Reference,
    ] {
        let server = server(&mock, profile, Some(SESSION));
        server
            .dispatch_tool(
                "write_session_note",
                args(json!({
                    "session_id": SOURCE, "note_uuid": "existing", "body": "Update"
                })),
            )
            .await?;
        server
            .dispatch_tool(
                "end_workspace_registration",
                args(json!({
                    "session_id": SOURCE
                })),
            )
            .await?;
    }
    mock.verify().await;
    Ok(())
}

#[test]
fn retrospective_instructions_and_capabilities_match_bounded_authority() -> anyhow::Result<()> {
    let server = McpServer::new(
        InspectClient::new("http://127.0.0.1:1".to_string()),
        Profile::Retrospective,
        Some(SESSION.to_string()),
    );
    let info = server.get_info();
    assert!(info.capabilities.experimental.is_none());
    let instructions = info
        .instructions
        .ok_or_else(|| anyhow::anyhow!("instructions missing"))?;
    assert!(instructions.contains("Unlisted tools are rejected"));
    assert!(!instructions.contains("MIKE_MCP_PROFILE=full"));
    Ok(())
}
