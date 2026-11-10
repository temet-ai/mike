use serde_json::{Map, Value, json};
use wiremock::matchers::{body_partial_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::client::InspectClient;

const DESCRIPTION_LIMIT: usize = 2048;

fn args(value: Value) -> Option<Map<String, Value>> {
    value.as_object().cloned()
}

async fn search_answering(server: &MockServer, detail: &str, answer: Value) {
    Mock::given(method("GET"))
        .and(path("/api/knowledge/search"))
        .and(query_param("detail", detail))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer))
        .expect(1)
        .mount(server)
        .await;
}

#[test]
fn search_and_similar_turn_tools_declare_detail_as_best_or_full() -> anyhow::Result<()> {
    let tools = crate::tools::all_tools();
    for name in ["search_project_knowledge", "recall_similar_turns"] {
        let tool = tools
            .iter()
            .find(|tool| tool.name == name)
            .ok_or_else(|| anyhow::anyhow!("{name} is not in the catalogue"))?;
        let declared = tool
            .input_schema
            .get("properties")
            .and_then(|properties| properties.get("detail"))
            .ok_or_else(|| anyhow::anyhow!("{name} does not declare detail"))?;
        assert_eq!(declared["enum"], json!(["best", "full"]), "{name}");
        let description = tool.description.as_deref().unwrap_or_default();
        assert!(description.contains("detail=full"), "{name}: {description}");
        assert!(description.chars().count() <= DESCRIPTION_LIMIT, "{name}");
    }
    Ok(())
}

#[tokio::test]
async fn search_project_knowledge_sends_detail_best_unless_the_caller_asks_for_full()
-> anyhow::Result<()> {
    let by_default = MockServer::start().await;
    search_answering(&by_default, "best", json!({"items": []})).await;
    let asked_full = MockServer::start().await;
    search_answering(&asked_full, "full", json!({"items": []})).await;

    crate::tools::call_tool(
        &InspectClient::new(by_default.uri()),
        "search_project_knowledge",
        args(json!({"project": "p", "query": "cache"})),
    )
    .await?;
    crate::tools::call_tool(
        &InspectClient::new(asked_full.uri()),
        "search_project_knowledge",
        args(json!({"project": "p", "query": "cache", "detail": "full"})),
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn recall_similar_turns_sends_detail_best_unless_the_caller_asks_for_full()
-> anyhow::Result<()> {
    for (sent, expected) in [
        (None, "best"),
        (Some("full"), "full"),
        (Some("best"), "best"),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/recall/similar-turns"))
            .and(body_partial_json(json!({"detail": expected})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"turns": []})))
            .expect(1)
            .mount(&server)
            .await;
        let mut arguments = json!({"project_path": "p", "query": "cache"});
        if let Some(sent) = sent {
            arguments["detail"] = json!(sent);
        }

        crate::tools::call_tool(
            &InspectClient::new(server.uri()),
            "recall_similar_turns",
            args(arguments),
        )
        .await?;
    }
    Ok(())
}

#[tokio::test]
async fn search_project_knowledge_select_of_every_reference_resolves_on_a_best_answer()
-> anyhow::Result<()> {
    let server = MockServer::start().await;
    let answer = json!({"items": [
        {"reference": "turn:1", "title": "first", "excerpt": "whole passage"},
        {"reference": "turn:2", "line": "turn \u{b7} 2026-11-09 \u{b7} second"},
        {"reference": "note:3", "line": "note \u{b7} 2026-11-08 \u{b7} third"}
    ], "more": {"shown_in_full": 1, "pointers": 2, "how": "read or repeat"}});
    search_answering(&server, "best", answer).await;

    let result = crate::tools::call_tool(
        &InspectClient::new(server.uri()),
        "search_project_knowledge",
        args(json!({"project": "p", "query": "cache",
                    "select": ["/knowledge/items/*/reference", "/knowledge/items/0/excerpt"]})),
    )
    .await?;

    let structured = result
        .structured_content
        .ok_or_else(|| anyhow::anyhow!("expected structured_content"))?;
    assert_eq!(
        structured["knowledge"]["items"],
        json!([
            {"reference": "turn:1", "excerpt": "whole passage"},
            {"reference": "turn:2"},
            {"reference": "note:3"}
        ])
    );
    assert!(structured.get("select_unmatched").is_none());
    Ok(())
}
