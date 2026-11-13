use crate::test_db;
use mike::store::persistence::knowledge_repo::KnowledgePersist as _;

use super::fixtures::{make_pool_and_persist, one_hot, seed_project, seed_session};
use anyhow::Context;
use chrono::{Duration as ChronoDuration, SubsecRound, Utc};
use mike::store::persistence::session_embeddings::SessionEmbeddingWrite;
use test_db::TestDb;

#[tokio::test]
async fn upsert_session_embedding_then_status_round_trips() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping upsert_session_embedding_then_status_round_trips"
        );
        return Ok(());
    };
    let test_db = factory.acquire_shared().await?;
    let (pool, persist) = make_pool_and_persist(&test_db.database_url).await?;

    let project_id = seed_project(
        &pool,
        &format!("upsert-status-test-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let now = Utc::now().trunc_subsecs(6);
    let (_pk, session_uuid) = seed_session(&pool, project_id, None, now).await?;

    assert!(
        persist
            .knowledge()
            .session_embedding_status(session_uuid)
            .await?
            .is_none()
    );

    let embedding = one_hot(5, 1.0);
    let source_updated_at = now - ChronoDuration::seconds(30);
    let before = Utc::now();
    persist
        .knowledge()
        .upsert_session_embedding(SessionEmbeddingWrite {
            session_id: session_uuid,
            embedding: &embedding,
            model: "test-model",
            source: "centroid",
            chunk_count: 3,
            source_updated_at,
        })
        .await?;

    let status = persist
        .knowledge()
        .session_embedding_status(session_uuid)
        .await?
        .context("row must exist after upsert")?;
    assert_eq!(status.source, "centroid");
    assert_eq!(status.model, "test-model");
    assert_eq!(status.chunk_count, 3);
    let skew_budget = ChronoDuration::seconds(30);
    assert!(
        status.embedded_at >= before - skew_budget && status.embedded_at <= before + skew_budget,
        "embedded_at {} must be within {skew_budget} of the upsert call at {before} \
         (clock-skew-tolerant bound, not sub-second)",
        status.embedded_at
    );

    let overview_embedding = one_hot(6, 1.0);
    persist
        .knowledge()
        .upsert_session_embedding(SessionEmbeddingWrite {
            session_id: session_uuid,
            embedding: &overview_embedding,
            model: "test-model",
            source: "overview",
            chunk_count: 0,
            source_updated_at: now,
        })
        .await?;
    let status2 = persist
        .knowledge()
        .session_embedding_status(session_uuid)
        .await?
        .context("row must still exist after second upsert")?;
    assert_eq!(status2.source, "overview");
    assert_eq!(status2.chunk_count, 0);

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memory.session_embeddings WHERE session_id = $1")
            .bind(session_uuid)
            .fetch_one(&pool)
            .await?;
    assert_eq!(count, 1, "upsert must replace, never duplicate, the row");

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
