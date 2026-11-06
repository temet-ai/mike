use super::*;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn arguments(value: Value) -> Option<Map<String, Value>> {
    value.as_object().cloned()
}

#[test]
fn the_three_everyday_tools_are_core_and_the_entry_tool_is_deferred() {
    for name in [TOOL_LIST_THREADS, TOOL_GET_THREAD, TOOL_CHECKPOINT_THREAD] {
        assert!(super::super::CORE_TOOL_NAMES.contains(&name), "{name}");
    }
    assert!(!super::super::CORE_TOOL_NAMES.contains(&TOOL_ADD_THREAD_ENTRY));
}

#[test]
fn every_thread_tool_takes_select_and_describes_itself_in_a_few_sentences() {
    for tool in work_threads_tools() {
        assert!(
            tool.input_schema["properties"]["select"].is_object(),
            "{}",
            tool.name
        );
        let description = tool.description.as_deref().unwrap_or_default();
        assert!(
            description.len() < 700,
            "{}: {} characters",
            tool.name,
            description.len()
        );
    }
}

#[test]
fn checkpoint_requires_the_slug_and_the_tldr_only() {
    let schema = checkpoint_thread_tool().input_schema;
    assert_eq!(schema["required"], json!(["slug", "tldr"]));
}

#[tokio::test]
async fn checkpoint_sends_session_id_as_session_uuid_and_leaves_unset_fields_out()
-> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/work-threads"))
        .and(body_json(json!({
            "project": "p", "slug": "s", "tldr": "t", "status": "review",
            "session_uuid": "00000000-0000-4000-8000-000000000001",
            "acceptance_criteria": "it works",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"created": false})))
        .expect(1)
        .mount(&server)
        .await;
    let client = InspectClient::new(server.uri());
    call_tool(
        &client,
        TOOL_CHECKPOINT_THREAD,
        arguments(json!({
            "project": "p", "slug": "s", "tldr": "t", "status": "review",
            "acceptance_criteria": "it works",
            "session_id": "00000000-0000-4000-8000-000000000001",
        })),
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn a_misspelled_checkpoint_field_is_refused_by_name() -> anyhow::Result<()> {
    let client = InspectClient::new("http://127.0.0.1:1".into());
    let refused = call_tool(
        &client,
        TOOL_CHECKPOINT_THREAD,
        arguments(json!({"project": "p", "slug": "s", "tldr": "t", "next": "later"})),
    )
    .await;
    let error = refused
        .err()
        .ok_or_else(|| anyhow::anyhow!("an unknown field was accepted"))?;
    assert!(
        error.message.contains("unknown field `next`"),
        "{}",
        error.message
    );
    Ok(())
}

#[tokio::test]
async fn get_thread_and_list_threads_need_a_project_and_forward_it() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/work-threads/s"))
        .and(query_param("project", "p"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"thread": {"slug": "s"}})))
        .expect(1)
        .mount(&server)
        .await;
    let client = InspectClient::new(server.uri());
    call_tool(
        &client,
        TOOL_GET_THREAD,
        arguments(json!({"project": "p", "slug": "s"})),
    )
    .await?;
    let refused = call_tool(&client, TOOL_LIST_THREADS, arguments(json!({}))).await;
    let error = refused
        .err()
        .ok_or_else(|| anyhow::anyhow!("work_thread_list without a project was accepted"))?;
    assert!(error.message.contains("needs `project`"));
    Ok(())
}
