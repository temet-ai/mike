use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

fn arguments(value: Value) -> Option<Map<String, Value>> {
    value.as_object().cloned()
}

#[test]
fn create_body_carries_the_patch_as_the_definition() {
    let body = create_body("night-run", "/repo", Some(json!({"goal": "g"})), None);
    assert_eq!(
        body,
        json!({"slug": "night-run", "project_path": "/repo", "definition": {"goal": "g"}})
    );
    assert_eq!(
        create_body("a", "/r", None, None),
        json!({"slug": "a", "project_path": "/r"})
    );
}

#[test]
fn create_body_records_the_calling_session_as_the_author() {
    let body = create_body(
        "a",
        "/r",
        None,
        Some("9dda3b31-f7e6-4547-9e13-33b566cfab97"),
    );
    assert_eq!(
        body,
        json!({"slug": "a", "project_path": "/r", "created_by": "9dda3b31-f7e6-4547-9e13-33b566cfab97"})
    );
}

#[tokio::test]
async fn define_fleet_creating_posts_the_author_session() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    let client = InspectClient::new(server.uri());
    Mock::given(method("POST"))
        .and(path("/api/fleets"))
        .and(body_json(json!({
            "slug": "a", "project_path": "/r",
            "created_by": "9dda3b31-f7e6-4547-9e13-33b566cfab97"
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"fleet": "f1"})))
        .expect(1)
        .mount(&server)
        .await;
    call_tool(
        &client,
        TOOL_DEFINE_FLEET,
        arguments(json!({
            "slug": "a", "project_path": "/r",
            "created_by": "9dda3b31-f7e6-4547-9e13-33b566cfab97"
        })),
    )
    .await?;
    Ok(())
}

#[test]
fn patch_body_names_only_what_was_given() {
    assert_eq!(patch_body(None, None), json!({}));
    assert_eq!(
        patch_body(Some(json!({"goal": null})), Some(true)),
        json!({"patch": {"goal": null}, "ready": true})
    );
    assert_eq!(patch_body(None, Some(false)), json!({"ready": false}));
}

#[test]
fn define_fleet_description_names_every_item_and_the_rules() {
    let tool = define_fleet_tool();
    let text = tool.description.as_deref().unwrap_or_default();
    for item in [
        "goal",
        "ask",
        "repo",
        "branch",
        "rules",
        "shared_files",
        "criteria",
        "end-to-end repo paths",
        "task owner",
        "no inference from prose",
        "coordinator",
        "observer",
        "acceptance",
        "workers",
        "tasks",
        "contract",
        "seats",
        "discipline",
        "after",
        "missing",
        "at_ready",
        "check",
        "reference",
        "as_notes",
        "goals",
        "/goal",
        "merges",
        "null",
        "mike starts it",
        "Never launch its seats yourself",
    ] {
        assert!(text.contains(item), "the description must mention {item}");
    }
}

#[test]
fn define_fleet_patch_parameter_names_the_decisions_keys() {
    let tool = define_fleet_tool();
    let patch = tool
        .input_schema
        .get("properties")
        .and_then(|p| p.get("patch"))
        .and_then(|p| p.get("description"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    for key in [
        "decisions",
        "stop_check",
        "wake_check",
        "triage",
        "commit_message",
        "effort",
    ] {
        assert!(
            patch.contains(key),
            "the patch description must mention {key}"
        );
    }
}

#[test]
fn both_tools_take_select_and_both_are_in_the_core_profile() {
    for tool in fleet_tools() {
        let properties = tool.input_schema.get("properties");
        assert!(
            properties.and_then(|p| p.get("select")).is_some(),
            "{} must take select",
            tool.name
        );
        assert!(
            super::super::CORE_TOOL_NAMES.contains(&tool.name.as_ref()),
            "{} must be in the core profile so every fleet seat sees it",
            tool.name
        );
    }
}

#[tokio::test]
async fn define_fleet_with_a_fleet_patches_it() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    let client = InspectClient::new(server.uri());
    Mock::given(method("PATCH"))
        .and(path("/api/fleets/night-run"))
        .and(body_json(json!({"patch": {"goal": "g"}, "ready": true})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"state": "draft"})))
        .expect(1)
        .mount(&server)
        .await;
    let result = call_tool(
        &client,
        TOOL_DEFINE_FLEET,
        arguments(json!({"fleet": "night-run", "patch": {"goal": "g"}, "ready": true})),
    )
    .await?;
    assert_eq!(
        result.structured_content,
        Some(json!({"fleet": {"state": "draft"}}))
    );
    Ok(())
}

#[tokio::test]
async fn define_fleet_without_a_fleet_creates_then_asks_ready_as_a_second_call()
-> anyhow::Result<()> {
    let server = MockServer::start().await;
    let client = InspectClient::new(server.uri());
    Mock::given(method("POST"))
        .and(path("/api/fleets"))
        .and(body_json(json!({
            "slug": "night-run", "project_path": "/repo", "definition": {"goal": "g"}
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"fleet": "f1"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/api/fleets/f1"))
        .and(body_json(json!({"ready": true})))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"fleet": "f1", "ready": false})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let result = call_tool(
        &client,
        TOOL_DEFINE_FLEET,
        arguments(json!({
            "slug": "night-run", "project_path": "/repo", "patch": {"goal": "g"}, "ready": true
        })),
    )
    .await?;
    assert_eq!(
        result.structured_content,
        Some(json!({"fleet": {"fleet": "f1", "ready": false}}))
    );
    Ok(())
}

#[tokio::test]
async fn define_fleet_without_ready_makes_only_the_create_call() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    let client = InspectClient::new(server.uri());
    Mock::given(method("POST"))
        .and(path("/api/fleets"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"fleet": "f1"})))
        .expect(1)
        .mount(&server)
        .await;
    call_tool(
        &client,
        TOOL_DEFINE_FLEET,
        arguments(json!({"slug": "a", "project_path": "/r"})),
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn define_fleet_that_fails_to_ask_ready_tells_the_session_the_id() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    let client = InspectClient::new(server.uri());
    Mock::given(method("POST"))
        .and(path("/api/fleets"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"fleet": "f1"})))
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/api/fleets/f1"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let failed = call_tool(
        &client,
        TOOL_DEFINE_FLEET,
        arguments(json!({"slug": "a", "project_path": "/r", "ready": true})),
    )
    .await;
    let message = failed
        .err()
        .map(|e| e.message.to_string())
        .unwrap_or_default();
    assert!(
        message.contains("`f1`") && message.contains("do not create"),
        "{message}"
    );
    Ok(())
}

#[tokio::test]
async fn define_fleet_creating_without_slug_or_path_is_an_invalid_params_error()
-> anyhow::Result<()> {
    let client = InspectClient::new("http://127.0.0.1:1".to_string());
    for args in [
        json!({"project_path": "/r"}),
        json!({"slug": "a"}),
        json!({"slug": " ", "project_path": "/r"}),
    ] {
        let failed = call_tool(&client, TOOL_DEFINE_FLEET, arguments(args)).await;
        let message = failed
            .err()
            .map(|e| e.message.to_string())
            .unwrap_or_default();
        assert!(message.contains("required to create a fleet"), "{message}");
    }
    Ok(())
}

#[tokio::test]
async fn get_fleet_reads_the_fleet() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    let client = InspectClient::new(server.uri());
    Mock::given(method("GET"))
        .and(path("/api/fleets/f1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"state": "running"})))
        .mount(&server)
        .await;
    let result = call_tool(&client, TOOL_GET_FLEET, arguments(json!({"fleet": "f1"}))).await?;
    assert_eq!(
        result.structured_content,
        Some(json!({"fleet": {"state": "running"}}))
    );
    Ok(())
}
