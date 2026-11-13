use crate::db_helpers;
use mike::store::persistence::knowledge_repo::KnowledgePersist as _;

use super::harness::{judge_response, start_proxy_with_state};
use super::seed::{count_lessons, planar_unit, seed_closed_turn, seed_project, seed_session};

use chrono::{Duration as ChronoDuration, Utc};
use mike::store::persistence::PersistHandle;
use mike::store::persistence::session_embeddings::SessionEmbeddingWrite;
use sqlx::PgPool;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn seed_neighbour_and_target(
    pool: &PgPool,
    persist: &PersistHandle,
    project_id: i64,
    now: chrono::DateTime<Utc>,
    old_enough: chrono::DateTime<Utc>,
) -> anyhow::Result<(Uuid, Uuid)> {
    let (_a_pk, a_uuid) = seed_session(
        pool,
        project_id,
        Some("Fix the mailbox stranding bug"),
        old_enough,
    )
    .await?;
    sqlx::query(
        "UPDATE sessions.sessions SET overview = $2, overview_generated_at = $3 WHERE session_uuid = $1",
    )
    .bind(a_uuid)
    .bind("Mailbox messages were stranding undelivered.\n\nThe delivery ack path was fixed.")
    .bind(now)
    .execute(pool)
    .await?;
    persist
        .knowledge()
        .upsert_session_embedding(SessionEmbeddingWrite {
            session_id: a_uuid,
            embedding: &planar_unit(0.0),
            model: "test-model",
            source: "overview",
            chunk_count: 1,
            source_updated_at: now,
        })
        .await?;
    persist
        .knowledge()
        .upsert_session_knowledge(
            a_uuid,
            &["mailbox".to_string()],
            &["mailbox delivery must ack before the sender clears".to_string()],
            &[],
            None,
        )
        .await?;

    let (b_pk, b_uuid) = seed_session(
        pool,
        project_id,
        Some("Harden mailbox delivery"),
        old_enough,
    )
    .await?;
    seed_closed_turn(
        pool,
        b_pk,
        project_id,
        "the mailbox ack sometimes races the sender clearing the message",
        "added a retry-safe ack path and a regression test",
    )
    .await?;
    persist
        .knowledge()
        .upsert_session_embedding(SessionEmbeddingWrite {
            session_id: b_uuid,
            embedding: &planar_unit(3.0),
            model: "test-model",
            source: "centroid",
            chunk_count: 1,
            source_updated_at: now,
        })
        .await?;

    Ok((a_uuid, b_uuid))
}

fn assert_judge_call_carries_related_block(call_body: &serde_json::Value) -> anyhow::Result<()> {
    let user_message = call_body["messages"][1]["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("user message content missing: {call_body}"))?;
    assert!(
        user_message.contains("RELATED SESSIONS"),
        "the related-sessions block must be present in the prompt; got: {user_message}"
    );
    assert!(
        user_message.contains("Fix the mailbox stranding bug"),
        "the neighbour's title must be present in the prompt; got: {user_message}"
    );
    assert!(
        user_message.contains("mailbox delivery must ack before the sender clears"),
        "the neighbour's learning must be present in the prompt; got: {user_message}"
    );
    Ok(())
}

async fn assert_knowledge_written_not_lessons(
    persist: &PersistHandle,
    pool: &PgPool,
    b_uuid: Uuid,
    lessons_before: i64,
) -> anyhow::Result<()> {
    let knowledge = persist
        .knowledge()
        .fetch_session_knowledge(b_uuid)
        .await?
        .ok_or_else(|| anyhow::anyhow!("session B must have a session_knowledge row"))?;
    assert_eq!(
        knowledge.topics,
        vec!["mailbox".to_string(), "delivery".to_string()]
    );
    assert_eq!(
        knowledge.learnings,
        vec!["ack-before-clear must hold across a retry".to_string()]
    );

    let lessons_after = count_lessons(pool).await?;
    assert_eq!(
        lessons_before, lessons_after,
        "the judge must never write to memory.lessons"
    );
    Ok(())
}

async fn assert_recorded_llm_call_carries_related_block(pool: &PgPool) -> anyhow::Result<()> {
    let recorded_body: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT request_body FROM llm.calls WHERE role = 'session_judge' ORDER BY id DESC LIMIT 1",
    )
    .fetch_one(pool)
    .await?;
    let recorded_body = recorded_body
        .ok_or_else(|| anyhow::anyhow!("the session judge's llm.calls row has no request_body"))?;
    let recorded_user_message = recorded_body["messages"][1]["content"]
        .as_str()
        .ok_or_else(|| {
            anyhow::anyhow!("recorded request_body has no messages[1].content: {recorded_body}")
        })?;
    assert!(
        recorded_user_message.contains("RELATED SESSIONS"),
        "the stored llm.calls row must carry the related-sessions block; got: {recorded_user_message}"
    );
    assert!(
        recorded_user_message.contains("Fix the mailbox stranding bug"),
        "the stored llm.calls row must carry the neighbour's title; got: {recorded_user_message}"
    );
    Ok(())
}

#[tokio::test]
async fn judge_composes_related_sessions_block_and_writes_knowledge_not_lessons()
-> anyhow::Result<()> {
    let upstream_mock = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream_mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    shutdown.cancel();

    let or_mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(judge_response(
            "success",
            "Continuing the mailbox stranding fix from \"Fix the mailbox stranding bug\", \
             this session hardened the delivery ack path.\n\n\
             The hardening shipped and the regression test passes.",
            &["mailbox", "delivery"],
            &["ack-before-clear must hold across a retry"],
        )))
        .mount(&or_mock)
        .await;

    let persist = PersistHandle::connect(&test_db.database_url).await?;
    let project_id = seed_project(&pool, "judge-related-sessions-test").await?;

    let now = Utc::now();
    let old_enough = now - ChronoDuration::hours(2);
    let (_a_uuid, b_uuid) =
        seed_neighbour_and_target(&pool, &persist, project_id, now, old_enough).await?;

    let lessons_before = count_lessons(&pool).await?;

    let (shutdown2, state) =
        start_proxy_with_state(&upstream_mock.uri(), &test_db.database_url, &or_mock.uri()).await?;

    mike::judge_worker::run_session_judge_once(&state).await?;

    let received = or_mock.received_requests().await.unwrap_or_default();
    let judge_call = received
        .iter()
        .find(|r| r.url.path() == "/chat/completions")
        .ok_or_else(|| anyhow::anyhow!("no /chat/completions request was recorded"))?;
    let call_body: serde_json::Value = serde_json::from_slice(&judge_call.body)?;
    assert_judge_call_carries_related_block(&call_body)?;

    assert_knowledge_written_not_lessons(&persist, &pool, b_uuid, lessons_before).await?;
    assert_recorded_llm_call_carries_related_block(&pool).await?;

    shutdown2.cancel();
    test_db.cleanup().await?;
    Ok(())
}
