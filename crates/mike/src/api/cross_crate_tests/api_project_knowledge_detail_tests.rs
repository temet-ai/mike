use serde_json::Value;

use crate::api::cross_crate_tests::api_project_knowledge_rerank_cache_tests::{
    embed_passages, references_of,
};
use crate::api::cross_crate_tests::api_project_knowledge_rerank_tests::seed_project_with_documents;
use crate::api::project_knowledge::best_shape::{FIRST_PASSAGE_CHARS, LINE_CHARS};
use crate::api::rerank_search::harness::Harness;

fn route(project: &str, extra: &str) -> String {
    format!("/api/knowledge/search?project={project}&query=cache{extra}")
}

fn field_names(value: &Value) -> Vec<String> {
    let mut names: Vec<String> = value
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default();
    names.sort();
    names
}

fn items_of(answer: &Value) -> Vec<Value> {
    answer["items"].as_array().cloned().unwrap_or_default()
}

#[tokio::test]
async fn knowledge_search_detail_best_gives_the_first_item_whole_and_the_rest_as_pointers()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;
    let references = references_of(&harness, &project).await?;
    embed_passages(&harness, &references, &"p".repeat(2_000)).await?;
    harness.enable_rerank(800);
    harness.reranker_prefers_later_documents(None).await;

    let (status, answer) = harness.get(&route(&project, "&detail=best")).await?;

    assert!(status.is_success(), "{answer}");
    let items = items_of(&answer);
    assert_eq!(items.len(), 3);
    for dropped in [
        "author",
        "rank",
        "lexical_rank",
        "rerank_score",
        "path",
        "copies",
    ] {
        assert!(items[0].get(dropped).is_none(), "{dropped}");
    }
    assert!(items[0].get("title").is_some() && items[0].get("reference").is_some());
    for dropped in ["read_tool", "retrieval_path"] {
        assert!(answer.get(dropped).is_none(), "{dropped}");
    }
    assert!(answer["rerank"].get("cached").is_none());
    assert_eq!(answer["rerank"]["state"], "reranked");
    if let Some(embedding) = answer.get("embedding") {
        assert_eq!(field_names(embedding), vec!["fallback"]);
    }
    assert_eq!(items[0]["excerpt_from"], "passage");
    assert_eq!(
        items[0]["excerpt"].as_str().map(|e| e.chars().count()),
        Some(FIRST_PASSAGE_CHARS)
    );
    for later in &items[1..] {
        assert_eq!(field_names(later), vec!["line", "reference"]);
        let line = later["line"].as_str().unwrap_or_default();
        assert!(line.chars().count() <= LINE_CHARS, "{line}");
        assert!(line.contains(" \u{b7} "), "{line}");
    }
    assert_eq!(answer["more"]["shown_in_full"], 1);
    assert_eq!(answer["more"]["pointers"], 2);
    assert!(
        answer["more"]["how"]
            .as_str()
            .is_some_and(|how| how.contains("detail=full"))
    );
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_without_detail_or_with_full_is_the_same_whole_answer()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;

    let (_, plain) = harness.get(&route(&project, "")).await?;
    let (_, full) = harness.get(&route(&project, "&detail=full")).await?;

    assert_eq!(plain, full);
    for item in items_of(&plain) {
        assert!(
            item.get("author").is_some() && item.get("line").is_none(),
            "{item}"
        );
    }
    assert!(plain.get("more").is_none());
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_detail_best_keeps_every_reference_at_its_position() -> anyhow::Result<()>
{
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;

    let (_, full) = harness.get(&route(&project, "")).await?;
    let (_, best) = harness.get(&route(&project, "&detail=best")).await?;

    let references = |answer: &Value| -> Vec<Value> {
        items_of(answer)
            .iter()
            .map(|item| item["reference"].clone())
            .collect()
    };
    assert_eq!(references(&best), references(&full));
    assert_eq!(references(&best).len(), 3);
    assert!(references(&best).iter().all(Value::is_string));
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_detail_best_is_recorded_from_the_whole_answer() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;
    harness.enable_rerank(800);
    harness.reranker_prefers_later_documents(None).await;

    harness.get(&route(&project, "&detail=best")).await?;

    let events = harness.events().await?;
    let returned = events
        .first()
        .and_then(|event| event.returned.as_array().cloned())
        .unwrap_or_default();
    assert_eq!(returned.len(), 3);
    assert!(
        returned
            .iter()
            .all(|entry| entry.get("rerank_score").is_some())
    );
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_detail_best_logs_the_first_passage_as_shown_and_nothing_for_the_rest()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;
    let references = references_of(&harness, &project).await?;
    embed_passages(&harness, &references, &"p".repeat(2_000)).await?;
    harness.enable_rerank(800);
    harness.reranker_prefers_later_documents(None).await;

    let (_, best) = harness.get(&route(&project, "&detail=best")).await?;

    let events = harness.events().await?;
    let returned = events
        .last()
        .and_then(|event| event.returned.as_array().cloned())
        .unwrap_or_default();
    assert_eq!(returned.len(), 3);
    assert_eq!(returned[0]["shown"], items_of(&best)[0]["excerpt"]);
    assert_eq!(
        returned[0]["shown"]
            .as_str()
            .map(|text| text.chars().count()),
        Some(FIRST_PASSAGE_CHARS)
    );
    assert!(
        returned[1..]
            .iter()
            .all(|entry| entry.get("shown").is_none())
    );
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_detail_full_logs_every_excerpt_as_shown() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;

    let (_, full) = harness.get(&route(&project, "&detail=full")).await?;

    let events = harness.events().await?;
    let returned = events
        .first()
        .and_then(|event| event.returned.as_array().cloned())
        .unwrap_or_default();
    let items = items_of(&full);
    assert_eq!(returned.len(), items.len());
    assert!(returned.iter().any(|entry| entry.get("shown").is_some()));
    for (entry, item) in returned.iter().zip(&items) {
        let excerpt = item["excerpt"]
            .as_str()
            .filter(|text| !text.trim().is_empty());
        assert_eq!(entry["shown"].as_str(), excerpt, "{entry}");
    }
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_with_another_detail_value_is_a_400_naming_both() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;

    let (status, answer) = harness.get(&route(&project, "&detail=brief")).await?;

    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
    let message = answer["error"].as_str().unwrap_or_default();
    assert!(
        message.contains("full") && message.contains("best"),
        "{message}"
    );
    harness.cleanup().await
}
