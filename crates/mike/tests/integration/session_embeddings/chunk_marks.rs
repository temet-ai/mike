use crate::test_db;

use super::fixtures::{
    make_pool_and_persist, one_hot, seed_chunk, seed_project, seed_session, seed_turn,
};
use anyhow::Context;
use chrono::{Duration as ChronoDuration, SubsecRound, Utc};
use mike::store::persistence::SessionEmbeddingWrite;
use test_db::TestDb;

#[tokio::test]
async fn session_chunk_mark_keeps_newest_chunk_time_when_an_older_chunk_lands_later()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping session_chunk_mark_keeps_newest");
        return Ok(());
    };
    let test_db = factory.acquire_shared().await?;
    let (pool, _persist) = make_pool_and_persist(&test_db.database_url).await?;

    let project_id =
        seed_project(&pool, &format!("chunk-mark-test-{}", uuid::Uuid::new_v4())).await?;
    let now = Utc::now().trunc_subsecs(6);
    let (session_pk, _) = seed_session(&pool, project_id, None, now).await?;
    let turn_id = seed_turn(&pool, session_pk, project_id).await?;
    let newer = now - ChronoDuration::seconds(10);
    seed_chunk(&pool, turn_id, 0, &one_hot(0, 1.0), newer).await?;
    seed_chunk(
        &pool,
        turn_id,
        1,
        &one_hot(1, 1.0),
        now - ChronoDuration::seconds(60),
    )
    .await?;

    let mark: chrono::DateTime<Utc> = sqlx::query_scalar(
        "SELECT chunks_embedded_at FROM memory.session_chunk_marks WHERE session_id = $1",
    )
    .bind(session_pk)
    .fetch_one(&pool)
    .await?;
    assert_eq!(mark, newer, "the mark only moves forward");

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn deleted_newest_chunk_does_not_make_the_session_a_candidate_again() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping deleted_newest_chunk_does_not_loop");
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;
    let (pool, persist) = make_pool_and_persist(&test_db.database_url).await?;

    let project_id = seed_project(&pool, "chunk-mark-loop-test").await?;
    let now = Utc::now().trunc_subsecs(6);
    let (session_pk, session_uuid) = seed_session(&pool, project_id, None, now).await?;
    let turn_id = seed_turn(&pool, session_pk, project_id).await?;
    seed_chunk(
        &pool,
        turn_id,
        0,
        &one_hot(0, 1.0),
        now - ChronoDuration::seconds(60),
    )
    .await?;
    let newest = seed_chunk(&pool, turn_id, 1, &one_hot(1, 1.0), now).await?;
    sqlx::query("DELETE FROM memory.operator_turn_chunks WHERE id = $1")
        .bind(newest)
        .execute(&pool)
        .await?;

    let candidates = persist.list_sessions_needing_embedding(10).await?;
    assert_eq!(
        candidates.iter().map(|c| c.session_id).collect::<Vec<_>>(),
        vec![session_uuid],
        "a session with chunks and no embedding is a candidate"
    );

    let (embedding, chunk_count, max_embedded_at) = persist
        .session_chunk_centroid(session_uuid)
        .await?
        .context("one chunk remains")?;
    assert_eq!(chunk_count, 1);
    assert_eq!(
        max_embedded_at, now,
        "the centroid reports the session mark, so the sweep records what it compares against"
    );
    persist
        .upsert_session_embedding(SessionEmbeddingWrite {
            session_id: session_uuid,
            embedding: &embedding,
            model: "test-model",
            source: "centroid",
            chunk_count: i32::try_from(chunk_count)?,
            source_updated_at: max_embedded_at,
        })
        .await?;

    assert!(
        persist
            .list_sessions_needing_embedding(10)
            .await?
            .is_empty(),
        "once embedded against the mark, the session is not served again"
    );

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn session_with_mark_but_no_chunks_is_not_a_candidate() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping session_with_mark_but_no_chunks");
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;
    let (pool, persist) = make_pool_and_persist(&test_db.database_url).await?;

    let project_id = seed_project(&pool, "chunk-mark-empty-test").await?;
    let now = Utc::now().trunc_subsecs(6);
    let (session_pk, _) = seed_session(&pool, project_id, None, now).await?;
    let turn_id = seed_turn(&pool, session_pk, project_id).await?;
    seed_chunk(&pool, turn_id, 0, &one_hot(0, 1.0), now).await?;
    sqlx::query("DELETE FROM memory.operator_turn_chunks WHERE turn_id = $1")
        .bind(turn_id)
        .execute(&pool)
        .await?;

    assert!(
        persist
            .list_sessions_needing_embedding(10)
            .await?
            .is_empty()
    );

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
