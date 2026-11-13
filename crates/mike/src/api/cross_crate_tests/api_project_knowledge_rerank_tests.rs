use crate::store::persistence::knowledge_repo::KnowledgePersist as _;

use std::time::Duration;

use serde_json::{Value, json};
use uuid::Uuid;

use crate::api::rerank_search::harness::{Harness, failing_answer, loading_answer, slow_answer};
use crate::store::persistence::project_knowledge::{
    DocumentInput, LocationInput, ScanInput, SourceInput,
};
use crate::store::persistence::test_support::seed_project;
use sha2::{Digest, Sha256};

fn document(path: &str, title: &str) -> DocumentInput {
    let body = format!("{title} explains how the cache stays warm");
    DocumentInput {
        relative_path: path.into(),
        title: title.into(),
        content_hash: Sha256::digest(body.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        body,
        kind: "design".into(),
        status: "accepted".into(),
        metadata: json!({}),
    }
}

pub(crate) async fn seed_project_with_documents(harness: &Harness) -> anyhow::Result<String> {
    let handle = &harness.handle;
    let project = handle
        .knowledge()
        .define_knowledge_project("Rerank", Uuid::new_v4())
        .await?;
    let project_uuid = project.project_uuid.to_string();
    let location = handle
        .knowledge()
        .register_project_location(&LocationInput {
            project: project_uuid.clone(),
            machine_uuid: Uuid::new_v4(),
            machine_name: "test".into(),
            root_path: "/work".into(),
        })
        .await?;
    let source = handle
        .knowledge()
        .register_knowledge_source(&SourceInput {
            location_uuid: serde_json::from_value(location["location_uuid"].clone())?,
            name: "artifacts".into(),
            root_path: "/work/artifacts".into(),
        })
        .await?;
    handle
        .knowledge()
        .ingest_knowledge_scan(
            serde_json::from_value(source["source_uuid"].clone())?,
            &ScanInput {
                generation: 0,
                documents: vec![
                    document("a.md", "Alpha"),
                    document("b.md", "Bravo"),
                    document("c.md", "Charlie"),
                ],
            },
        )
        .await?;
    Ok(project_uuid)
}

fn references(answer: &Value) -> Vec<String> {
    answer["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["reference"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

async fn plain_order(harness: &Harness, project: &str) -> anyhow::Result<Vec<String>> {
    let (status, answer) = harness
        .get(&format!(
            "/api/knowledge/search?project={project}&query=cache"
        ))
        .await?;
    anyhow::ensure!(status.is_success(), "plain search: {status}: {answer}");
    assert_eq!(answer["rerank"]["state"], "off");
    Ok(references(&answer))
}

fn search_route(project: &str, extra: &str) -> String {
    format!("/api/knowledge/search?project={project}&query=cache{extra}")
}

#[tokio::test]
async fn knowledge_search_with_rerank_enabled_reorders_items_and_reports_reranked()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;
    let plain = plain_order(&harness, &project).await?;
    assert_eq!(plain.len(), 3);
    harness.enable_rerank(800);
    harness.reranker_prefers_later_documents(None).await;

    let (status, answer) = harness.get(&search_route(&project, "")).await?;

    assert!(status.is_success(), "{answer}");
    let expected: Vec<String> = plain.iter().rev().cloned().collect();
    assert_eq!(references(&answer), expected);
    assert_eq!(answer["rerank"]["state"], "reranked");
    assert_eq!(answer["rerank"]["ready_in_s"], 0);
    assert_eq!(answer["rerank"]["left_out"], 0);
    assert!(answer["rerank"].get("detail").is_none());
    assert!(
        answer["items"][0]["rerank_score"]
            .as_f64()
            .is_some_and(|s| s > 0.0)
    );
    let sent = harness.rerank_requests().await?;
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["documents"].as_array().map(Vec::len), Some(3));
    assert!(
        sent[0]["documents"][0]
            .as_str()
            .is_some_and(|d| ["Alpha", "Bravo", "Charlie"].iter().any(|t| d.contains(t)))
    );
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_with_an_offset_pages_through_the_reranked_order() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;
    let plain = plain_order(&harness, &project).await?;
    harness.enable_rerank(800);
    harness.reranker_prefers_later_documents(None).await;

    let (_, answer) = harness
        .get(&search_route(&project, "&limit=1&offset=1"))
        .await?;

    assert_eq!(references(&answer), vec![plain[1].clone()]);
    assert_eq!(answer["has_more"], true);
    assert_eq!(answer["next_offset"], 2);
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_with_a_reranker_that_is_loading_keeps_plain_order() -> anyhow::Result<()>
{
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;
    let plain = plain_order(&harness, &project).await?;
    harness.enable_rerank(800);
    harness.reranker_answers(loading_answer()).await;

    let (_, answer) = harness.get(&search_route(&project, "")).await?;

    assert_eq!(references(&answer), plain);
    assert_eq!(answer["rerank"]["state"], "loading");
    assert_eq!(answer["rerank"]["ready_in_s"], 4);
    assert_eq!(
        answer["rerank"]["detail"],
        "the reranker is loading, ready in about 4 s; these results are in plain order"
    );
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_when_a_cold_reranker_times_out_reports_loading_in_plain_order()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;
    let plain = plain_order(&harness, &project).await?;
    harness.enable_rerank(50);
    harness
        .reranker_answers(slow_answer(Duration::from_secs(5)))
        .await;

    let (_, answer) = harness.get(&search_route(&project, "")).await?;

    assert_eq!(references(&answer), plain);
    assert_eq!(answer["rerank"]["state"], "loading");
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_when_a_warm_reranker_times_out_reports_unavailable_in_plain_order()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;
    let plain = plain_order(&harness, &project).await?;
    harness.enable_rerank(100);
    harness.reranker_prefers_later_documents(Some(1)).await;
    let (_, first) = harness.get(&search_route(&project, "")).await?;
    assert_eq!(first["rerank"]["state"], "reranked");
    harness
        .reranker_answers(slow_answer(Duration::from_secs(5)))
        .await;

    let (_, answer) = harness.get(&search_route(&project, "s")).await?;

    assert_eq!(references(&answer), plain);
    assert_eq!(answer["rerank"]["state"], "unavailable");
    assert!(
        answer["rerank"]["detail"].as_str().is_some_and(|d| d
            .contains("did not answer within 100 ms")
            && d.ends_with("plain order"))
    );
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_when_the_reranker_fails_reports_unavailable_in_plain_order()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;
    let plain = plain_order(&harness, &project).await?;
    harness.enable_rerank(800);
    harness.reranker_answers(failing_answer()).await;

    let (status, answer) = harness.get(&search_route(&project, "")).await?;

    assert!(status.is_success());
    assert_eq!(references(&answer), plain);
    assert_eq!(answer["rerank"]["state"], "unavailable");
    assert!(answer["rerank"]["ready_in_s"].is_null());
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_with_rerank_disabled_reports_off_and_calls_nothing() -> anyhow::Result<()>
{
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;
    harness.reranker_prefers_later_documents(None).await;

    let (_, answer) = harness.get(&search_route(&project, "")).await?;

    assert_eq!(answer["rerank"]["state"], "off");
    assert!(
        answer["rerank"]["detail"]
            .as_str()
            .is_some_and(|d| d.contains("rerank.enabled"))
    );
    assert!(harness.rerank_requests().await?.is_empty());
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_records_one_event_with_the_returned_order_and_session()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;
    let plain = plain_order(&harness, &project).await?;
    harness.enable_rerank(800);
    harness.reranker_prefers_later_documents(None).await;
    let owner = seed_project(harness.handle.pool(), "/rerank/owner").await?;
    let session = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, now(), now())",
    )
    .bind(session)
    .bind(owner)
    .execute(harness.handle.pool())
    .await?;

    let (_, answer) = harness
        .get(&search_route(
            &project,
            &format!("&kind=artifact&session_id={session}"),
        ))
        .await?;

    let events = harness.events().await?;
    assert_eq!(events.len(), 2);
    let event = &events[1];
    assert_eq!(event.tool, "search_project_knowledge");
    assert_eq!(event.query, "cache");
    assert_eq!(event.rerank_state, "reranked");
    let returned: Vec<&str> = event
        .returned
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|r| r["reference"].as_str())
                .collect()
        })
        .unwrap_or_default();
    let expected: Vec<&str> = plain.iter().rev().map(String::as_str).collect();
    assert_eq!(returned, expected);
    assert_eq!(references(&answer).len(), 3);
    assert!(event.returned[0]["rerank_score"].as_f64().is_some());
    assert!(event.session_id.is_some());
    assert_eq!(events[0].session_id, None);
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_with_an_unknown_session_id_records_a_null_session() -> anyhow::Result<()>
{
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;

    let (status, _) = harness
        .get(&search_route(
            &project,
            &format!("&session_id={}", Uuid::new_v4()),
        ))
        .await?;

    assert!(status.is_success());
    let events = harness.events().await?;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].session_id, None);
    assert_eq!(events[0].rerank_state, "off");
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_with_fewer_words_than_the_setting_keeps_plain_order_and_says_why()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;
    let plain = plain_order(&harness, &project).await?;
    harness.enable_rerank(800);
    harness
        .state
        .rerank_service()
        .apply_setting("rerank.min_query_words", &json!(3));
    harness.reranker_prefers_later_documents(None).await;

    let (status, answer) = harness
        .get(&format!(
            "/api/knowledge/search?project={project}&query=cache%20warm"
        ))
        .await?;

    assert!(status.is_success(), "{answer}");
    assert_eq!(references(&answer), plain);
    assert_eq!(answer["rerank"]["state"], "off");
    assert!(
        answer["rerank"]["detail"]
            .as_str()
            .is_some_and(|d| { d.contains("fewer than 3 words") && d.ends_with("plain order") })
    );
    assert!(harness.rerank_requests().await?.is_empty());
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_with_a_one_word_query_is_reranked_by_default() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;
    harness.enable_rerank(800);
    harness.reranker_prefers_later_documents(None).await;

    let (_, answer) = harness.get(&search_route(&project, "")).await?;

    assert_eq!(answer["rerank"]["state"], "reranked");
    assert_eq!(harness.rerank_requests().await?.len(), 1);
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_with_the_setting_back_at_one_reranks_a_short_query() -> anyhow::Result<()>
{
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;
    harness.enable_rerank(800);
    let service = harness.state.rerank_service();
    service.apply_setting("rerank.min_query_words", &json!(3));
    service.apply_setting("rerank.min_query_words", &json!(1));
    harness.reranker_prefers_later_documents(None).await;

    let (_, answer) = harness.get(&search_route(&project, "")).await?;

    assert_eq!(answer["rerank"]["state"], "reranked");
    harness.cleanup().await
}
