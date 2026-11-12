//! Moved from crates/mike/src/api/handlers/recall_turns_rerank_tests.rs by the crate split: it reaches the web
//! crate, so it is tested from there.

use serde_json::{Value, json};
use uuid::Uuid;

use crate::api::rerank_search::harness::{Harness, loading_answer};
use crate::store::persistence::test_support::{seed_project, seed_session_turns};

const ROUTE: &str = "/api/recall/similar-turns";

struct Seeded {
    project: String,
    session: Uuid,
    turn_ids: Vec<i64>,
}

async fn seed_turns(harness: &Harness) -> anyhow::Result<Seeded> {
    let pool = harness.handle.pool();
    let project = format!("/recall-rerank/{}", Uuid::new_v4());
    let project_id = seed_project(pool, &project).await?;
    let session = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, now(), now())",
    )
    .bind(session)
    .bind(project_id)
    .execute(pool)
    .await?;
    seed_session_turns(pool, &session.to_string(), project_id, 3).await?;
    sqlx::query(
        "UPDATE memory.operator_turns
            SET user_prompt = 'quicksort question ' || id,
                final_assistant_text = 'answer about quicksort ' || id",
    )
    .execute(pool)
    .await?;
    let turn_ids = sqlx::query_scalar("SELECT id FROM memory.operator_turns ORDER BY id")
        .fetch_all(pool)
        .await?;
    Ok(Seeded {
        project,
        session,
        turn_ids,
    })
}

fn turn_ids(answer: &Value) -> Vec<i64> {
    answer["turns"]
        .as_array()
        .map(|turns| turns.iter().filter_map(|t| t["turn_id"].as_i64()).collect())
        .unwrap_or_default()
}

async fn recall(harness: &Harness, seeded: &Seeded, limit: Option<u64>) -> anyhow::Result<Value> {
    let (status, answer) = harness
        .post(
            ROUTE,
            json!({"project": seeded.project, "query": "quicksort", "limit": limit,
                   "session_id": seeded.session, "include_own_turn": true}),
        )
        .await?;
    anyhow::ensure!(status.is_success(), "recall: {status}: {answer}");
    Ok(answer)
}

#[tokio::test]
async fn recall_similar_turns_with_rerank_enabled_reorders_turns_and_reports_reranked()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed_turns(&harness).await?;
    let plain = turn_ids(&recall(&harness, &seeded, None).await?);
    assert_eq!(plain.len(), 3);
    harness.enable_rerank(800);
    harness.reranker_prefers_later_documents(None).await;

    let answer = recall(&harness, &seeded, None).await?;

    let expected: Vec<i64> = plain.iter().rev().copied().collect();
    assert_eq!(turn_ids(&answer), expected);
    assert_eq!(answer["rerank"]["state"], "reranked");
    assert!(answer["turns"][0]["rerank_score"].as_f64().is_some());
    let sent = harness.rerank_requests().await?;
    assert_eq!(sent[0]["documents"].as_array().map(Vec::len), Some(3));
    assert!(
        sent[0]["documents"][0].as_str().is_some_and(
            |d| d.contains("quicksort question") && d.contains("answer about quicksort")
        )
    );
    harness.cleanup().await
}

#[tokio::test]
async fn recall_similar_turns_sends_documents_that_begin_with_the_turn_header() -> anyhow::Result<()>
{
    let harness = Harness::boot().await?;
    let seeded = seed_turns(&harness).await?;
    sqlx::query(
        "UPDATE sessions.sessions SET title = 'Quicksort research' WHERE session_uuid = $1",
    )
    .bind(seeded.session)
    .execute(harness.handle.pool())
    .await?;
    harness.enable_rerank(800);
    harness.reranker_prefers_later_documents(None).await;

    let answer = recall(&harness, &seeded, None).await?;

    let sent = harness.rerank_requests().await?;
    let documents = sent[0]["documents"].as_array().cloned().unwrap_or_default();
    assert_eq!(documents.len(), 3);
    for document in documents.iter().filter_map(Value::as_str) {
        let header = document.lines().next().unwrap_or_default();
        assert!(header.starts_with("turn | "), "{header}");
        assert!(header.ends_with(" | Quicksort research"), "{header}");
        assert!(document.contains("quicksort question"));
        assert!(document.chars().count() <= crate::analysis::rerank::MAX_DOCUMENT_CHARS);
    }
    assert!(!answer.to_string().contains("turn | "));
    harness.cleanup().await
}

#[tokio::test]
async fn recall_similar_turns_without_a_session_title_sends_a_two_part_header() -> anyhow::Result<()>
{
    let harness = Harness::boot().await?;
    let seeded = seed_turns(&harness).await?;
    harness.enable_rerank(800);
    harness.reranker_prefers_later_documents(None).await;

    recall(&harness, &seeded, None).await?;

    let sent = harness.rerank_requests().await?;
    let documents = sent[0]["documents"].as_array().cloned().unwrap_or_default();
    assert_eq!(documents.len(), 3);
    for document in documents.iter().filter_map(Value::as_str) {
        let header = document.lines().next().unwrap_or_default();
        assert_eq!(header.split(" | ").count(), 2, "{header}");
    }
    harness.cleanup().await
}

#[tokio::test]
async fn recall_similar_turns_with_a_limit_cuts_the_reranked_order_to_the_page()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed_turns(&harness).await?;
    let plain = turn_ids(&recall(&harness, &seeded, None).await?);
    harness.enable_rerank(800);
    harness.reranker_prefers_later_documents(None).await;

    let answer = recall(&harness, &seeded, Some(1)).await?;

    assert_eq!(turn_ids(&answer), vec![plain[2]]);
    assert_eq!(
        harness.rerank_requests().await?[0]["documents"]
            .as_array()
            .map(Vec::len),
        Some(3)
    );
    harness.cleanup().await
}

#[tokio::test]
async fn recall_similar_turns_with_a_loading_reranker_keeps_plain_order() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed_turns(&harness).await?;
    let plain = turn_ids(&recall(&harness, &seeded, None).await?);
    harness.enable_rerank(800);
    harness.reranker_answers(loading_answer()).await;

    let answer = recall(&harness, &seeded, None).await?;

    assert_eq!(turn_ids(&answer), plain);
    assert_eq!(answer["rerank"]["state"], "loading");
    assert_eq!(answer["rerank"]["ready_in_s"], 4);
    harness.cleanup().await
}

#[tokio::test]
async fn recall_similar_turns_with_fewer_words_than_the_setting_keeps_plain_order()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed_turns(&harness).await?;
    let plain = turn_ids(&recall(&harness, &seeded, None).await?);
    harness.enable_rerank(800);
    harness
        .state
        .rerank_service()
        .apply_setting("rerank.min_query_words", &json!(3));
    harness.reranker_prefers_later_documents(None).await;

    let answer = recall(&harness, &seeded, None).await?;

    assert_eq!(turn_ids(&answer), plain);
    assert_eq!(answer["rerank"]["state"], "off");
    assert!(
        answer["rerank"]["detail"]
            .as_str()
            .is_some_and(|d| d.contains("fewer than 3 words"))
    );
    assert!(harness.rerank_requests().await?.is_empty());
    harness.cleanup().await
}

#[tokio::test]
async fn recall_similar_turns_with_rerank_disabled_reports_off_and_calls_nothing()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed_turns(&harness).await?;
    harness.reranker_prefers_later_documents(None).await;

    let answer = recall(&harness, &seeded, None).await?;

    assert_eq!(answer["rerank"]["state"], "off");
    assert!(answer["turns"][0].get("rerank_score").is_none());
    assert!(harness.rerank_requests().await?.is_empty());
    harness.cleanup().await
}

#[tokio::test]
async fn recall_similar_turns_records_one_event_per_call_with_turn_references() -> anyhow::Result<()>
{
    let harness = Harness::boot().await?;
    let seeded = seed_turns(&harness).await?;
    harness.enable_rerank(800);
    harness.reranker_prefers_later_documents(None).await;

    let answer = recall(&harness, &seeded, Some(2)).await?;

    let events = harness.events().await?;
    assert_eq!(events.len(), 1);
    let event = &events[0];
    assert_eq!(event.tool, "recall_similar_turns");
    assert_eq!(event.query, "quicksort");
    assert_eq!(event.rerank_state, "reranked");
    assert!(event.session_id.is_some());
    let returned: Vec<String> = event
        .returned
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|r| r["reference"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let expected: Vec<String> = turn_ids(&answer)
        .iter()
        .map(|id| format!("turn:{id}"))
        .collect();
    assert_eq!(returned, expected);
    assert!(
        turn_ids(&answer)
            .iter()
            .all(|id| seeded.turn_ids.contains(id))
    );
    harness.cleanup().await
}
