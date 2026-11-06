use super::*;
use serde_json::json;
use wiremock::matchers::{body_partial_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SESSION: &str = "00000000-0000-4000-8000-000000000001";

fn args(value: Value) -> Option<Map<String, Value>> {
    value.as_object().cloned()
}

#[tokio::test]
async fn caller_defaults_fill_only_the_expected_identity_field() -> anyhow::Result<()> {
    let client = InspectClient::new("http://127.0.0.1:1".into());
    for (tool, field) in [
        ("ask", "from"),
        ("answer", "from"),
        ("decline", "from"),
        ("write_session_note", "session_id"),
    ] {
        let result = supply_defaults(
            &client,
            tool,
            args(json!({"select": ["/note"]})),
            Some(SESSION),
        )
        .await?;
        let result = result.ok_or_else(|| anyhow::anyhow!("missing arguments"))?;
        assert_eq!(result[field], SESSION);
        assert_eq!(result["select"], json!(["/note"]));
        assert_eq!(result.len(), 2);
    }
    Ok(())
}

#[tokio::test]
async fn explicit_arguments_are_preserved_without_lookup_or_caller() -> anyhow::Result<()> {
    let client = InspectClient::new("http://127.0.0.1:1".into());
    for (tool, field) in [
        ("ask", "from"),
        ("answer", "from"),
        ("decline", "from"),
        ("write_session_note", "session_id"),
        ("read_project_knowledge", "project"),
    ] {
        for value in [json!("explicit"), Value::Null, json!("")] {
            let original = Some(Map::from_iter([(field.to_string(), value)]));
            let result = supply_defaults(&client, tool, original.clone(), Some(SESSION)).await?;
            assert_eq!(result, original);
        }
    }
    assert_eq!(
        supply_defaults(&client, "get_session", None, None).await?,
        None
    );
    Ok(())
}

#[tokio::test]
async fn absent_or_invalid_caller_rejects_content_based_impersonation() -> anyhow::Result<()> {
    let client = InspectClient::new("http://127.0.0.1:1".into());
    for session in [
        None,
        Some(""),
        Some("arbitrary"),
        Some("00000000-0000-4000-8000-not-a-uuid00"),
    ] {
        for tool in [
            "ask",
            "answer",
            "decline",
            "write_session_note",
            "read_project_knowledge",
        ] {
            let result = supply_defaults(
                &client,
                tool,
                args(json!({"body": SESSION, "question": SESSION, "session": SESSION})),
                session,
            )
            .await;
            let error = result
                .err()
                .ok_or_else(|| anyhow::anyhow!("untrusted text supplied identity"))?;
            assert!(error.message.contains("trusted caller context"));
            assert!(error.message.contains("CLAUDE_CODE_SESSION_ID"));
        }
    }
    Ok(())
}

#[tokio::test]
async fn caller_project_comes_from_session_summary() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/sessions/{SESSION}")))
        .and(query_param("limit", "1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"summary": {"id": SESSION, "project": "/verified/project"}})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/knowledge/read"))
        .and(query_param("project", "/verified/project"))
        .and(query_param("reference", "artifact:1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"body": "source"})))
        .expect(1)
        .mount(&server)
        .await;
    let client = InspectClient::new(server.uri());
    let prepared = supply_defaults(
        &client,
        "read_project_knowledge",
        args(json!({"reference": "artifact:1"})),
        Some(SESSION),
    )
    .await?;
    crate::tools::call_tool(&client, "read_project_knowledge", prepared).await?;
    Ok(())
}

#[tokio::test]
async fn caller_project_missing_or_failed_lookup_is_an_error() {
    for response in [
        ResponseTemplate::new(404),
        ResponseTemplate::new(200)
            .set_body_json(json!({"summary": {"id": SESSION, "project": null}})),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(response)
            .mount(&server)
            .await;
        let client = InspectClient::new(server.uri());
        assert!(
            supply_defaults(&client, "read_project_knowledge", None, Some(SESSION))
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn caller_defaults_reach_question_http_body() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/questions"))
        .and(body_partial_json(
            json!({"from": SESSION, "to": "reference", "question": "Where?"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"question_id": "1"})))
        .expect(1)
        .mount(&server)
        .await;
    let client = InspectClient::new(server.uri());
    let prepared = supply_defaults(
        &client,
        "ask",
        args(json!({"to": "reference", "question": "Where?"})),
        Some(SESSION),
    )
    .await?;
    crate::tools::call_tool(&client, "ask", prepared).await?;
    Ok(())
}

#[tokio::test]
async fn caller_defaults_reach_answer_and_note_http_requests() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/questions/q1/answer"))
        .and(body_partial_json(
            json!({"from": SESSION, "answer": "Here."}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"state": "answered"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/api/sessions/{SESSION}/notes")))
        .and(body_partial_json(
            json!({"title": "Finding", "body": "Evidence"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"note_uuid": "n1"})))
        .expect(1)
        .mount(&server)
        .await;
    let client = InspectClient::new(server.uri());
    for (tool, arguments) in [
        ("answer", json!({"question_id": "q1", "answer": "Here."})),
        (
            "write_session_note",
            json!({"title": "Finding", "body": "Evidence"}),
        ),
    ] {
        let prepared = supply_defaults(&client, tool, args(arguments), Some(SESSION)).await?;
        crate::tools::call_tool(&client, tool, prepared).await?;
    }
    Ok(())
}

#[tokio::test]
async fn caller_project_refuses_a_different_session_summary() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"summary": {"id": "another-session", "project": "/other/project"}}),
        ))
        .mount(&server)
        .await;
    let client = InspectClient::new(server.uri());
    assert!(
        supply_defaults(&client, "read_project_knowledge", None, Some(SESSION))
            .await
            .is_err()
    );
}

#[test]
fn caller_default_schemas_do_not_require_resolvable_fields() -> anyhow::Result<()> {
    let tools = crate::tools::tools_for_profile(crate::Profile::Full);
    for (name, field) in [
        ("ask", "from"),
        ("answer", "from"),
        ("decline", "from"),
        ("write_session_note", "session_id"),
        ("read_project_knowledge", "project"),
    ] {
        let tool = tools
            .iter()
            .find(|tool| tool.name == name)
            .ok_or_else(|| anyhow::anyhow!("tool {name} is not registered"))?;
        assert!(
            !tool
                .input_schema
                .get("required")
                .and_then(Value::as_array)
                .is_some_and(|fields| fields.iter().any(|value| value == field))
        );
        assert!(
            tool.input_schema["properties"][field]["description"]
                .as_str()
                .is_some_and(|description| description.contains("caller"))
        );
    }
    Ok(())
}

#[tokio::test]
async fn caller_defaults_reach_the_thread_checkpoint_body() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/sessions/{SESSION}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"summary": {"id": SESSION, "project": "/verified/project"}})),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/work-threads"))
        .and(body_partial_json(json!({
            "project": "/verified/project",
            "session_uuid": SESSION,
            "slug": "motion",
            "tldr": "where it stands",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"created": true})))
        .expect(1)
        .mount(&server)
        .await;
    let client = InspectClient::new(server.uri());
    let prepared = supply_defaults(
        &client,
        "work_thread_checkpoint",
        args(json!({"slug": "motion", "tldr": "where it stands"})),
        Some(SESSION),
    )
    .await?;
    crate::tools::call_tool(&client, "work_thread_checkpoint", prepared).await?;
    Ok(())
}

#[tokio::test]
async fn thread_writes_without_a_caller_keep_their_arguments_and_reads_need_a_project()
-> anyhow::Result<()> {
    let client = InspectClient::new("http://127.0.0.1:1".into());
    let explicit = json!({"project": "p", "slug": "s", "kind": "note", "body": "b"});
    let prepared = supply_defaults(
        &client,
        "work_thread_add_entry",
        args(explicit.clone()),
        None,
    )
    .await?;
    assert_eq!(prepared, args(explicit));
    let refused = supply_defaults(&client, "work_thread_list", None, None).await;
    let error = refused.err().ok_or_else(|| {
        anyhow::anyhow!("work_thread_list without a project or a caller was accepted")
    })?;
    assert!(error.message.contains("requires `project`"));
    Ok(())
}
