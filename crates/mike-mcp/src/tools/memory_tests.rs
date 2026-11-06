use super::*;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[test]
fn record_lesson_schema_exposes_optional_shared_topic_and_persona_scope() {
    let schema = record_lesson_tool().input_schema;
    assert_eq!(schema["properties"]["topic"]["type"], "string");
    assert!(
        schema["properties"]["topic"]["description"]
            .as_str()
            .is_some_and(|text| text.contains("shared:<topic>"))
    );
    assert!(
        schema["properties"]["agent_name"]["description"]
            .as_str()
            .is_some_and(|text| text.contains("not primary sessions"))
    );
    assert!(
        !schema["required"]
            .as_array()
            .is_some_and(|fields| fields.iter().any(|field| field == "topic"))
    );
}

#[tokio::test]
async fn record_lesson_tool_forwards_shared_topic_verbatim() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    let arguments = json!({"project_path":"project","kind":"convention","text":"Reuse checked results","topic":"shared:verification"});
    Mock::given(method("POST"))
        .and(path("/api/lessons"))
        .and(body_json(arguments.clone()))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"id":9,"topic":"shared:verification"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let result = call_tool(
        &InspectClient::new(server.uri()),
        TOOL_RECORD_LESSON,
        arguments.as_object().cloned(),
    )
    .await?;
    assert_eq!(
        result.structured_content,
        Some(json!({"lesson":{"id":9,"topic":"shared:verification"}}))
    );
    Ok(())
}
