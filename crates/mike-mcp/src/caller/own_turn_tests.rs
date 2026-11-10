use super::*;
use serde_json::json;
use wiremock::matchers::{body_partial_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SESSION: &str = "00000000-0000-4000-8000-000000000001";

fn args(value: Value) -> Option<Map<String, Value>> {
    value.as_object().cloned()
}

#[test]
fn the_three_search_tools_declare_include_own_turn_as_a_boolean() -> anyhow::Result<()> {
    let tools = crate::tools::all_tools();
    for name in [
        "search_project_knowledge",
        "recall_similar_turns",
        "recall_project_context",
    ] {
        let tool = tools
            .iter()
            .find(|tool| tool.name == name)
            .ok_or_else(|| anyhow::anyhow!("{name} is not in the catalogue"))?;
        let declared = tool
            .input_schema
            .get("properties")
            .and_then(|properties| properties.get("include_own_turn"))
            .ok_or_else(|| anyhow::anyhow!("{name} does not declare include_own_turn"))?;
        assert_eq!(declared["type"], "boolean", "{name}");
    }
    Ok(())
}

#[tokio::test]
async fn recall_project_context_is_tagged_with_the_caller_session_and_forwards_include_own_turn()
-> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/recall/project-context"))
        .and(query_param("session_id", SESSION))
        .and(query_param("include_own_turn", "true"))
        .and(query_param("query", "cache"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"lessons": []})))
        .expect(1)
        .mount(&server)
        .await;
    let client = InspectClient::new(server.uri());

    let prepared = supply_defaults(
        &client,
        "recall_project_context",
        args(json!({"project_path": "p", "query": "cache", "include_own_turn": true})),
        Some(SESSION),
    )
    .await?;

    let tagged = prepared
        .as_ref()
        .and_then(|arguments| arguments.get("session_id"));
    assert_eq!(tagged, Some(&json!(SESSION)));
    crate::tools::call_tool(&client, "recall_project_context", prepared).await?;
    Ok(())
}

#[tokio::test]
async fn recall_project_context_without_a_caller_session_sends_no_session_id() -> anyhow::Result<()>
{
    let client = InspectClient::new("http://127.0.0.1:1".into());
    let original = args(json!({"project_path": "p", "query": "cache"}));

    let prepared =
        supply_defaults(&client, "recall_project_context", original.clone(), None).await?;

    assert_eq!(prepared, original);
    Ok(())
}

#[tokio::test]
async fn recall_similar_turns_forwards_include_own_turn_in_the_body() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/recall/similar-turns"))
        .and(body_partial_json(
            json!({"session_id": SESSION, "include_own_turn": true}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"turns": []})))
        .expect(1)
        .mount(&server)
        .await;
    let client = InspectClient::new(server.uri());

    let prepared = supply_defaults(
        &client,
        "recall_similar_turns",
        args(json!({"project_path": "p", "query": "cache", "include_own_turn": true})),
        Some(SESSION),
    )
    .await?;

    crate::tools::call_tool(&client, "recall_similar_turns", prepared).await?;
    Ok(())
}

#[tokio::test]
async fn search_project_knowledge_forwards_include_own_turn_in_the_query() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/knowledge/search"))
        .and(query_param("session_id", SESSION))
        .and(query_param("include_own_turn", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": []})))
        .expect(1)
        .mount(&server)
        .await;
    let client = InspectClient::new(server.uri());

    let prepared = supply_defaults(
        &client,
        "search_project_knowledge",
        args(json!({"project": "p", "query": "cache", "include_own_turn": true})),
        Some(SESSION),
    )
    .await?;

    crate::tools::call_tool(&client, "search_project_knowledge", prepared).await?;
    Ok(())
}
