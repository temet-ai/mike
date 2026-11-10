use super::*;
use serde_json::json;
use wiremock::matchers::{body_partial_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn args(value: Value) -> Option<Map<String, Value>> {
    value.as_object().cloned()
}

#[test]
fn search_and_similar_turn_tools_declare_order_as_relevance_or_recent() -> anyhow::Result<()> {
    let tools = crate::tools::all_tools();
    for name in ["search_project_knowledge", "recall_similar_turns"] {
        let tool = tools
            .iter()
            .find(|tool| tool.name == name)
            .ok_or_else(|| anyhow::anyhow!("{name} is not in the catalogue"))?;
        let declared = tool
            .input_schema
            .get("properties")
            .and_then(|properties| properties.get("order"))
            .ok_or_else(|| anyhow::anyhow!("{name} does not declare order"))?;
        assert_eq!(declared["type"], "string", "{name}");
        assert_eq!(declared["enum"], json!(["relevance", "recent"]), "{name}");
        let description = tool.description.as_deref().unwrap_or_default();
        assert!(
            description.contains("order=recent"),
            "{name}: {description}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn search_project_knowledge_forwards_order_in_the_query() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/knowledge/search"))
        .and(query_param("order", "recent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": []})))
        .expect(1)
        .mount(&server)
        .await;
    let client = InspectClient::new(server.uri());

    crate::tools::call_tool(
        &client,
        "search_project_knowledge",
        args(json!({"project": "p", "query": "cache", "order": "recent"})),
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn recall_similar_turns_forwards_order_in_the_body() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/recall/similar-turns"))
        .and(body_partial_json(json!({"order": "recent"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"turns": []})))
        .expect(1)
        .mount(&server)
        .await;
    let client = InspectClient::new(server.uri());

    crate::tools::call_tool(
        &client,
        "recall_similar_turns",
        args(json!({"project_path": "p", "query": "cache", "order": "recent"})),
    )
    .await?;
    Ok(())
}
