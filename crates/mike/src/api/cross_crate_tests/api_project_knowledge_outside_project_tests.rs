//! Moved from crates/mike/src/api/project_knowledge/outside_project_tests.rs by the crate split: it reaches the web
//! crate, so it is tested from there.

use serde_json::{Value, json};

use crate::api::cross_crate_tests::api_project_knowledge_rerank_tests::seed_project_with_documents;
use crate::api::cross_crate_tests::api_project_knowledge_wide_pools_tests::seed_unassigned_document;
use crate::api::rerank_search::harness::Harness;
use crate::store::persistence::project_knowledge::KnowledgeQuery;

struct Seeded {
    project: String,
    outside_reference: String,
}

async fn seed(harness: &Harness) -> anyhow::Result<Seeded> {
    let project = seed_project_with_documents(harness).await?;
    let sentinel = seed_unassigned_document(harness).await?;
    let rows = harness
        .handle
        .knowledge_candidate_rows(
            sentinel.id,
            &KnowledgeQuery {
                project: sentinel.project_uuid.to_string(),
                query: "cache".into(),
                ..KnowledgeQuery::default()
            },
            10,
            None,
        )
        .await?;
    let outside_reference = rows
        .first()
        .and_then(|row| row["reference"].as_str())
        .ok_or_else(|| anyhow::anyhow!("the (no project) document was not found: {rows:?}"))?
        .to_owned();
    Ok(Seeded {
        project,
        outside_reference,
    })
}

fn read_route(project: &str, reference: &str) -> String {
    format!("/api/knowledge/read?project={project}&reference={reference}")
}

fn search_route(project: &str, extra: &str) -> String {
    format!("/api/knowledge/search?project={project}&query=cache{extra}")
}

fn items_of(answer: &Value) -> Vec<Value> {
    answer["items"].as_array().cloned().unwrap_or_default()
}

fn outside_items(answer: &Value) -> Vec<Value> {
    items_of(answer)
        .into_iter()
        .filter(|item| item["provenance"]["outside_project"] == true)
        .collect()
}

#[tokio::test]
async fn knowledge_read_of_a_reference_from_the_no_project_pool_opens_it_and_says_so()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;

    let (status, read) = harness
        .get(&read_route(&seeded.project, &seeded.outside_reference))
        .await?;

    assert!(status.is_success(), "{read}");
    assert_eq!(read["outside_project"], true);
    assert_eq!(read["body"], "Delta explains how the cache stays warm");
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_read_of_a_reference_in_neither_project_stays_a_409() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;

    let (status, answer) = harness
        .get(&read_route(&seeded.project, "artifact:987654321"))
        .await?;

    assert_eq!(status, reqwest::StatusCode::CONFLICT);
    assert_eq!(
        answer["error"],
        "Knowledge reference not found in this project"
    );
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_read_of_an_outside_reference_with_the_setting_off_stays_a_409()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;
    harness
        .state
        .rerank_service()
        .apply_setting("rerank.search_no_project", &json!(false));

    let (status, _) = harness
        .get(&read_route(&seeded.project, &seeded.outside_reference))
        .await?;

    assert_eq!(status, reqwest::StatusCode::CONFLICT);
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_read_of_a_reference_of_the_named_project_is_not_marked_outside()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;
    let (_, search) = harness.get(&search_route(&seeded.project, "")).await?;
    let own = items_of(&search)
        .into_iter()
        .find(|item| item["provenance"]["outside_project"] != true)
        .and_then(|item| item["reference"].as_str().map(str::to_owned))
        .ok_or_else(|| anyhow::anyhow!("no project item: {search}"))?;

    let (status, read) = harness.get(&read_route(&seeded.project, &own)).await?;

    assert!(status.is_success(), "{read}");
    assert!(read.get("outside_project").is_none());
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_reranked_marks_the_outside_item_and_orders_it_after_project_items()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;
    harness.enable_rerank(800);
    harness.reranker_prefers_later_documents(None).await;

    let (_, answer) = harness.get(&search_route(&seeded.project, "")).await?;

    let items = items_of(&answer);
    assert_eq!(items.len(), 4);
    let outside = outside_items(&answer);
    assert_eq!(outside.len(), 1);
    assert_eq!(outside[0]["reference"], json!(seeded.outside_reference));
    assert_eq!(items[3]["reference"], json!(seeded.outside_reference));
    assert!(
        outside[0]["rerank_score"]
            .as_f64()
            .is_some_and(|s| s > 0.0 && s < 1.0)
    );
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_detail_best_ends_the_outside_pointer_line_with_the_suffix()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;
    harness.enable_rerank(800);
    harness.reranker_prefers_later_documents(None).await;

    let (_, answer) = harness
        .get(&search_route(&seeded.project, "&detail=best"))
        .await?;

    let items = items_of(&answer);
    let last = items.last().cloned().unwrap_or(Value::Null);
    assert_eq!(last["reference"], json!(seeded.outside_reference));
    assert!(
        last["line"]
            .as_str()
            .is_some_and(|line| line.ends_with(" \u{b7} outside this project")),
        "{last}"
    );
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_with_the_setting_off_returns_only_project_items_unmarked()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;
    harness
        .state
        .rerank_service()
        .apply_setting("rerank.search_no_project", &json!(false));
    harness.enable_rerank(800);
    harness.reranker_prefers_later_documents(None).await;

    let (_, answer) = harness.get(&search_route(&seeded.project, "")).await?;

    assert_eq!(items_of(&answer).len(), 3);
    assert!(outside_items(&answer).is_empty());
    harness.cleanup().await
}
