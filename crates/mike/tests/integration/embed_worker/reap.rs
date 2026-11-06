use crate::test_db::{self, TestDb};

use mike::store::persistence::PersistHandle;
use sqlx::PgPool;

async fn seed_open_turn(
    pool: &PgPool,
    project_id: i64,
    last_seen: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<i64> {
    let session_uuid = uuid::Uuid::new_v4();
    sqlx::query!(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, $3, $3)",
        session_uuid,
        project_id,
        last_seen,
    )
    .execute(pool)
    .await?;

    let request_id: i64 = sqlx::query_scalar!(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming,
            message_count, is_tool_loop, status_id,
            params, last_user_message, human_prompt,
            turn_type_id, turn_origin_id, created_at, completed_at
         )
         SELECT s.id, $2, rm.id, '/v1/messages', false,
                0, false, rs.id, '{}'::jsonb, '', 'seed prompt', rtt.id, rto.id,
                $3, $3
           FROM sessions.sessions s,
                requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto
          WHERE s.session_uuid = $1
            AND rm.code = 'POST' AND rs.code = 'complete'
            AND rtt.code = 'user_prompt' AND rto.code = 'primary'
         RETURNING id",
        session_uuid,
        project_id,
        last_seen,
    )
    .fetch_one(pool)
    .await?;

    let turn_id: i64 = sqlx::query_scalar!(
        "INSERT INTO memory.operator_turns (
            session_id, project_id, start_request_id, end_request_id, user_prompt
         )
         SELECT s.id, $2, $3, $3, 'seed prompt'
           FROM sessions.sessions s
          WHERE s.session_uuid = $1
         RETURNING id",
        session_uuid,
        project_id,
        request_id,
    )
    .fetch_one(pool)
    .await?;

    Ok(turn_id)
}

#[tokio::test]
async fn reap_closes_turn_idle_past_threshold() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = PgPool::connect(&db.database_url).await?;

    let project_id = test_db::seed_project(&pool, "embed-worker-reap-idle").await?;
    let now = chrono::Utc::now();
    let idle_threshold = std::time::Duration::from_secs(30 * 60);
    let turn_id = seed_open_turn(&pool, project_id, now - chrono::Duration::minutes(40)).await?;

    let reaped = handle.reap_idle_open_turns(idle_threshold, now).await?;
    assert_eq!(reaped, vec![turn_id], "the idle turn must be reaped");

    let closed_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar!(
        "SELECT closed_at FROM memory.operator_turns WHERE id = $1",
        turn_id,
    )
    .fetch_one(&pool)
    .await?;
    assert!(closed_at.is_some(), "closed_at must be stamped");

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn reap_does_not_close_turn_within_threshold() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = PgPool::connect(&db.database_url).await?;

    let project_id = test_db::seed_project(&pool, "embed-worker-reap-fresh").await?;
    let now = chrono::Utc::now();
    let idle_threshold = std::time::Duration::from_secs(30 * 60);
    let turn_id = seed_open_turn(&pool, project_id, now - chrono::Duration::minutes(5)).await?;

    let reaped = handle.reap_idle_open_turns(idle_threshold, now).await?;
    assert!(
        reaped.is_empty(),
        "a session active within the threshold must not be reaped"
    );

    let closed_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar!(
        "SELECT closed_at FROM memory.operator_turns WHERE id = $1",
        turn_id,
    )
    .fetch_one(&pool)
    .await?;
    assert!(closed_at.is_none(), "the fresh turn must remain open");

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn reap_is_idempotent_on_rerun() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = PgPool::connect(&db.database_url).await?;

    let project_id = test_db::seed_project(&pool, "embed-worker-reap-idempotent").await?;
    let now = chrono::Utc::now();
    let idle_threshold = std::time::Duration::from_secs(30 * 60);
    let turn_id = seed_open_turn(&pool, project_id, now - chrono::Duration::minutes(40)).await?;

    let first = handle.reap_idle_open_turns(idle_threshold, now).await?;
    assert_eq!(first, vec![turn_id]);

    let second = handle
        .reap_idle_open_turns(idle_threshold, now + chrono::Duration::minutes(1))
        .await?;
    assert!(
        second.is_empty(),
        "re-running the reaper against an already-closed turn must be a no-op"
    );

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn reaped_turn_is_visible_to_embed_candidate_query() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = PgPool::connect(&db.database_url).await?;

    let project_id = test_db::seed_project(&pool, "embed-worker-reap-visible").await?;
    let now = chrono::Utc::now();
    let idle_threshold = std::time::Duration::from_secs(30 * 60);
    let turn_id = seed_open_turn(&pool, project_id, now - chrono::Duration::minutes(40)).await?;

    let before = handle.fetch_unembedded_turn_ids(10).await?;
    assert!(
        !before.contains(&turn_id),
        "an open turn must not be embed-eligible yet"
    );

    let reaped = handle.reap_idle_open_turns(idle_threshold, now).await?;
    assert_eq!(reaped, vec![turn_id]);

    let after = handle.fetch_unembedded_turn_ids(10).await?;
    assert!(
        after.contains(&turn_id),
        "a reaped (closed) turn must be visible to the embed worker's candidate query"
    );

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn health_counts_open_and_closed_unembedded_separately() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = PgPool::connect(&db.database_url).await?;

    let project_id = test_db::seed_project(&pool, "embed-worker-health-split").await?;
    let now = chrono::Utc::now();
    let idle_threshold = std::time::Duration::from_secs(30 * 60);
    let turn_id = seed_open_turn(&pool, project_id, now - chrono::Duration::minutes(40)).await?;

    let open_before = handle.count_open_unembedded_turns().await?;
    let closed_before = handle.count_pending_embedding_turns().await?;
    assert!(open_before >= 1, "the seeded open turn must be counted");

    handle.reap_idle_open_turns(idle_threshold, now).await?;

    let open_after = handle.count_open_unembedded_turns().await?;
    let closed_after = handle.count_pending_embedding_turns().await?;
    assert_eq!(
        open_after,
        open_before - 1,
        "reaping the turn must decrement the open-unembedded count"
    );
    assert_eq!(
        closed_after,
        closed_before + 1,
        "reaping the turn must increment the closed-unembedded count"
    );

    let closed_ids = handle.fetch_unembedded_turn_ids(1000).await?;
    assert!(closed_ids.contains(&turn_id));

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
