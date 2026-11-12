//! Moved from crates/mike/src/api/project_knowledge/rerank_header_tests.rs by the crate split: it reaches the web
//! crate, so it is tested from there.

use serde_json::Value;

use crate::analysis::rerank::MAX_DOCUMENT_CHARS;
use crate::api::cross_crate_tests::api_project_knowledge_rerank_cache_tests::{
    embed_passages, references_of,
};
use crate::api::cross_crate_tests::api_project_knowledge_rerank_tests::seed_project_with_documents;
use crate::api::rerank_search::harness::Harness;

const HEADER_MARK: &str = "artifact | ";

fn route(project: &str, extra: &str) -> String {
    format!("/api/knowledge/search?project={project}&query=cache{extra}")
}

async fn sent_documents(harness: &Harness) -> anyhow::Result<Vec<String>> {
    let sent = harness.rerank_requests().await?;
    let first = sent
        .first()
        .ok_or_else(|| anyhow::anyhow!("the reranker received no request"))?;
    Ok(first["documents"]
        .as_array()
        .map(|documents| {
            documents
                .iter()
                .filter_map(|d| d.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default())
}

fn excerpts(answer: &Value) -> Vec<String> {
    answer["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["excerpt"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

async fn seeded(harness: &Harness) -> anyhow::Result<String> {
    let project = seed_project_with_documents(harness).await?;
    let references = references_of(harness, &project).await?;
    embed_passages(harness, &references, &"p".repeat(2_000)).await?;
    harness.enable_rerank(800);
    harness.reranker_prefers_later_documents(None).await;
    Ok(project)
}

#[tokio::test]
async fn knowledge_search_sends_the_reranker_documents_that_begin_with_the_item_header()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seeded(&harness).await?;

    let (status, answer) = harness.get(&route(&project, "")).await?;

    assert!(status.is_success(), "{answer}");
    let documents = sent_documents(&harness).await?;
    assert_eq!(documents.len(), 3);
    for document in &documents {
        let mut lines = document.lines();
        let header = lines.next().unwrap_or_default();
        let title = lines.next().unwrap_or_default();
        assert!(header.starts_with(HEADER_MARK), "{header}");
        assert!(header.ends_with(".md"), "{header}");
        assert_eq!(header.split(" | ").count(), 3, "{header}");
        assert!(["Alpha", "Bravo", "Charlie"].contains(&title), "{title}");
        assert!(document.chars().count() <= MAX_DOCUMENT_CHARS);
    }
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_returns_excerpts_and_the_search_log_without_the_header()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seeded(&harness).await?;

    let (_, answer) = harness.get(&route(&project, "")).await?;
    let (_, best) = harness.get(&route(&project, "&detail=best")).await?;

    assert!(sent_documents(&harness).await?[0].starts_with(HEADER_MARK));
    let shown: Vec<String> = excerpts(&answer)
        .into_iter()
        .chain(excerpts(&best))
        .collect();
    assert!(shown.len() >= 4, "{shown:?}");
    assert!(shown.iter().all(|e| !e.contains(HEADER_MARK)), "{shown:?}");
    assert!(
        best["items"][0]["excerpt"]
            .as_str()
            .is_some_and(|e| e.starts_with("pppp"))
    );
    let events = harness.events().await?;
    assert!(!events.is_empty());
    assert!(
        events
            .iter()
            .all(|event| !event.returned.to_string().contains(HEADER_MARK))
    );
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_detail_best_lines_start_with_the_item_label() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seeded(&harness).await?;

    let (_, best) = harness.get(&route(&project, "&detail=best")).await?;

    let lines: Vec<&str> = best["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["line"].as_str())
        .collect();
    assert_eq!(lines.len(), 2);
    assert!(
        lines
            .iter()
            .all(|line| line.starts_with("artifact \u{b7} ")),
        "{lines:?}"
    );
    harness.cleanup().await
}
