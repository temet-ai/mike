use serde_json::Value;

use super::rerank_tests::seed_project_with_documents;
use crate::api::rerank_search::harness::Harness;
use crate::store::operator_turns::vector_to_text_literal;

pub(super) const PASSAGE_MODEL: &str = "test-model";

fn axis_zero() -> Vec<f32> {
    let mut vector = vec![0.0; 768];
    vector[0] = 1.0;
    vector
}

pub(super) async fn references_of(harness: &Harness, project: &str) -> anyhow::Result<Vec<String>> {
    let (_, answer) = harness
        .get(&format!(
            "/api/knowledge/search?project={project}&query=cache"
        ))
        .await?;
    Ok(answer["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["reference"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default())
}

pub(super) async fn embed_passages(
    harness: &Harness,
    references: &[String],
    body: &str,
) -> anyhow::Result<()> {
    for reference in references {
        let revision: i64 = reference.trim_start_matches("artifact:").parse()?;
        let state: i64 = sqlx::query_scalar(
            "INSERT INTO memory.knowledge_embedding_state
                 (source_kind, source_id, model_key, completed_at)
             VALUES ('artifact', $1, $2, now()) RETURNING id",
        )
        .bind(revision)
        .bind(PASSAGE_MODEL)
        .fetch_one(harness.handle.pool())
        .await?;
        sqlx::query(
            "INSERT INTO memory.knowledge_embedding_chunks (state_id, ordinal, body, embedding)
             VALUES ($1, 0, $2, $3::text::public.vector)",
        )
        .bind(state)
        .bind(format!("{body} [{reference}]"))
        .bind(vector_to_text_literal(&axis_zero()))
        .execute(harness.handle.pool())
        .await?;
    }
    Ok(())
}

fn search_route(project: &str) -> String {
    format!("/api/knowledge/search?project={project}&query=cache")
}

#[tokio::test]
async fn knowledge_search_repeated_reranked_search_makes_no_second_request_and_reports_cached()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;
    harness.enable_rerank(800);
    harness.reranker_prefers_later_documents(None).await;

    let (_, first) = harness.get(&search_route(&project)).await?;
    let (_, second) = harness.get(&search_route(&project)).await?;

    assert_eq!(harness.rerank_requests().await?.len(), 1);
    assert_eq!(first["rerank"]["cached"], 0);
    assert_eq!(second["rerank"]["cached"], 3);
    assert_eq!(second["rerank"]["state"], "reranked");
    assert_eq!(second["items"], first["items"]);
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_with_the_reranker_off_reports_no_cached_figure() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;

    let (_, answer) = harness.get(&search_route(&project)).await?;

    assert_eq!(answer["rerank"]["state"], "off");
    assert!(answer["rerank"].get("cached").is_none());
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_reranked_item_with_a_passage_shows_it_as_the_excerpt()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;
    let references = references_of(&harness, &project).await?;
    embed_passages(&harness, &references, "The chunk that matched the question").await?;
    harness.enable_rerank(800);
    harness.reranker_prefers_later_documents(None).await;

    let (_, answer) = harness.get(&search_route(&project)).await?;

    let items = answer["items"].as_array().cloned().unwrap_or_default();
    assert_eq!(items.len(), 3);
    for item in &items {
        let reference = item["reference"].as_str().unwrap_or_default();
        assert_eq!(
            item["excerpt"],
            format!("The chunk that matched the question [{reference}]")
        );
        assert_eq!(item["excerpt_from"], "passage");
    }
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_without_the_reranker_keeps_the_stored_excerpt() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;
    let references = references_of(&harness, &project).await?;
    embed_passages(&harness, &references, "The chunk that matched the question").await?;

    let (_, answer) = harness.get(&search_route(&project)).await?;

    for item in answer["items"].as_array().cloned().unwrap_or_default() {
        assert!(item.get("excerpt_from").is_none(), "{item}");
        assert_ne!(item["excerpt"], Value::Null);
    }
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_two_searches_inside_the_lifetime_run_the_coverage_count_once()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;

    let (_, first) = harness.get(&search_route(&project)).await?;
    let (_, second) = harness.get(&search_route(&project)).await?;

    assert_eq!(harness.handle.knowledge_memos().counts_fetched().1, 1);
    assert_eq!(
        first["embedding"]["coverage"],
        second["embedding"]["coverage"]
    );
    let id = harness.handle.resolve_knowledge_project(&project).await?.id;
    let key = crate::embed_worker::knowledge::model_key(&harness.state.embedding_service());
    let direct = harness
        .handle
        .knowledge_embedding_health(Some(id), &key)
        .await?;
    assert_eq!(first["embedding"]["coverage"], direct);
    harness.cleanup().await
}
