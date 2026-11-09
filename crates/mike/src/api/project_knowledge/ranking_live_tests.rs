use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::api::rerank_search::harness::Harness;
use crate::store::persistence::project_knowledge::{
    DocumentInput, LocationInput, ScanInput, SourceInput,
};

fn document(path: &str, title: &str, body: &str) -> DocumentInput {
    DocumentInput {
        relative_path: path.into(),
        title: title.into(),
        content_hash: Sha256::digest(body.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        body: body.into(),
        kind: "design".into(),
        status: "accepted".into(),
        metadata: json!({}),
    }
}

async fn scan(
    harness: &Harness,
    location: &Value,
    name: &str,
    documents: Vec<DocumentInput>,
) -> anyhow::Result<()> {
    let source = harness
        .handle
        .register_knowledge_source(&SourceInput {
            location_uuid: serde_json::from_value(location["location_uuid"].clone())?,
            name: name.into(),
            root_path: format!("/work/{name}"),
        })
        .await?;
    harness
        .handle
        .ingest_knowledge_scan(
            serde_json::from_value(source["source_uuid"].clone())?,
            &ScanInput {
                generation: 0,
                documents,
            },
        )
        .await?;
    Ok(())
}

async fn seed_mixed_kinds(harness: &Harness) -> anyhow::Result<String> {
    let project = harness
        .handle
        .define_knowledge_project("Ranking", Uuid::new_v4())
        .await?;
    let project_uuid = project.project_uuid.to_string();
    let location = harness
        .handle
        .register_project_location(&LocationInput {
            project: project_uuid.clone(),
            machine_uuid: Uuid::new_v4(),
            machine_name: "test".into(),
            root_path: "/work".into(),
        })
        .await?;
    let plain = "explains how the cache stays warm";
    scan(
        harness,
        &location,
        "artifacts",
        vec![
            document("a.md", "Alpha", &format!("Alpha {plain}")),
            document("b.md", "Bravo", &format!("Bravo {plain}")),
        ],
    )
    .await?;
    scan(
        harness,
        &location,
        "claude-memory",
        vec![document(
            "m.md",
            "Memory",
            "Memory cache cache cache cache cache cache",
        )],
    )
    .await?;
    Ok(project_uuid)
}

fn kinds(answer: &Value) -> Vec<String> {
    answer["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["source_kind"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn route(project: &str) -> String {
    format!("/api/knowledge/search?project={project}&query=cache")
}

#[tokio::test]
async fn knowledge_search_with_a_kind_weight_of_zero_moves_that_kind_last_without_the_reranker()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seed_mixed_kinds(&harness).await?;
    let (_, plain) = harness.get(&route(&project)).await?;
    assert_eq!(kinds(&plain).first().map(String::as_str), Some("memory"));
    harness
        .state
        .rerank_service()
        .apply_setting("search.ranking.kind_weights", &json!({"memory": 0.0}));

    let (status, answer) = harness.get(&route(&project)).await?;

    assert!(status.is_success(), "{answer}");
    assert_eq!(kinds(&answer).last().map(String::as_str), Some("memory"));
    assert_eq!(kinds(&answer).len(), 3);
    assert_eq!(answer["rerank"]["state"], "off");
    assert!(harness.rerank_requests().await?.is_empty());
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_with_the_default_ranking_keeps_the_plain_order() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seed_mixed_kinds(&harness).await?;
    let (_, before) = harness.get(&route(&project)).await?;
    let service = harness.state.rerank_service();
    service.apply_setting("search.ranking.kind_weights", &json!({"memory": 0.0}));
    service.apply_setting("search.ranking.kind_weights", &json!({}));

    let (_, after) = harness.get(&route(&project)).await?;

    assert_eq!(after["items"], before["items"]);
    assert_eq!(after["rerank"], before["rerank"]);
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_with_a_changed_ranking_sends_the_reordered_candidates_to_the_reranker()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seed_mixed_kinds(&harness).await?;
    harness.enable_rerank(800);
    harness.reranker_prefers_later_documents(None).await;
    harness
        .state
        .rerank_service()
        .apply_setting("search.ranking.kind_weights", &json!({"memory": 0.0}));

    let (_, answer) = harness.get(&route(&project)).await?;

    assert_eq!(answer["rerank"]["state"], "reranked");
    assert_eq!(kinds(&answer).first().map(String::as_str), Some("memory"));
    let sent = harness.rerank_requests().await?;
    assert_eq!(sent.len(), 1);
    let last = sent[0]["documents"]
        .as_array()
        .and_then(|d| d.last())
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(last.contains("Memory"), "{last}");
    harness.cleanup().await
}
