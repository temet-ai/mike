use chrono::{Duration, Utc};
use sqlx::PgPool;

use super::CandidateWindow;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support as test_util;

async fn seed_closed_turn(pool: &PgPool, path: &str) -> anyhow::Result<(i64, i64)> {
    let project_id = test_util::seed_project(pool, path).await?;
    let session_id: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES (gen_random_uuid(), $1, NOW(), NOW()) RETURNING id",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    let request_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
             session_id, project_id, method_id, path, is_streaming, message_count,
             is_tool_loop, status_id, params, last_user_message, human_prompt,
             turn_type_id, turn_origin_id, created_at, completed_at)
         SELECT $1, $2, rm.id, '/v1/messages', true, 1, false, rs.id, '{}'::jsonb, '', '',
                rtt.id, rto.id, NOW(), NOW()
           FROM requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto
          WHERE rm.code = 'POST' AND rs.code = 'complete'
            AND rtt.code = 'user_prompt' AND rto.code = 'primary'
          RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    let turn_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turns
             (session_id, project_id, start_request_id, end_request_id, outcome_id, closed_at)
         SELECT $1, $2, $3, $3, tot.id, NOW() FROM memory.turn_outcomes tot
          WHERE tot.code = 'unknown'
         RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(request_id)
    .fetch_one(pool)
    .await?;
    Ok((session_id, turn_id))
}

fn window() -> CandidateWindow {
    let now = Utc::now();
    CandidateWindow {
        lookback_from: now - Duration::hours(1),
        settled_before: now,
        limit: 25,
    }
}

async fn candidates(handle: &PersistHandle) -> anyhow::Result<Vec<i64>> {
    Ok(handle
        .find_turn_facts_candidates("turn-facts-test", window())
        .await?)
}

async fn make_due(pool: &PgPool, turn_id: i64, attempt_count: i32) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE memory.turn_facts_backstop_attempts
            SET retry_after = now() - interval '1 second', attempt_count = $2
          WHERE operator_turn_id = $1",
    )
    .bind(turn_id)
    .bind(attempt_count)
    .execute(pool)
    .await?;
    Ok(())
}

async fn attempt_row(
    pool: &PgPool,
    turn_id: i64,
) -> anyhow::Result<Option<(i32, Option<chrono::DateTime<Utc>>)>> {
    Ok(sqlx::query_as(
        "SELECT attempt_count, unbuildable_at
           FROM memory.turn_facts_backstop_attempts WHERE operator_turn_id = $1",
    )
    .bind(turn_id)
    .fetch_optional(pool)
    .await?)
}

#[tokio::test]
async fn find_candidates_served_turn_backs_off_and_other_sessions_still_served()
-> anyhow::Result<()> {
    let Some(db) = test_util::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (stuck_session, stuck_turn) = seed_closed_turn(&handle.pool, "/tmp/backstop-a").await?;

    assert_eq!(candidates(&handle).await?, vec![stuck_session]);
    assert_eq!(
        attempt_row(&handle.pool, stuck_turn).await?,
        Some((1, None))
    );
    assert!(candidates(&handle).await?.is_empty());

    let (fresh_session, _) = seed_closed_turn(&handle.pool, "/tmp/backstop-b").await?;
    assert_eq!(candidates(&handle).await?, vec![fresh_session]);

    make_due(&handle.pool, stuck_turn, 1).await?;
    assert_eq!(candidates(&handle).await?, vec![stuck_session]);
    assert_eq!(
        attempt_row(&handle.pool, stuck_turn).await?,
        Some((2, None))
    );
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn find_candidates_last_allowed_attempt_marks_turn_unbuildable() -> anyhow::Result<()> {
    let Some(db) = test_util::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (session, turn) = seed_closed_turn(&handle.pool, "/tmp/backstop-unbuildable").await?;
    assert_eq!(candidates(&handle).await?, vec![session]);

    make_due(&handle.pool, turn, super::BACKSTOP_MAX_ATTEMPTS - 1).await?;
    assert_eq!(candidates(&handle).await?, vec![session]);
    let Some((count, unbuildable_at)) = attempt_row(&handle.pool, turn).await? else {
        anyhow::bail!("the served turn has no attempt row");
    };
    assert_eq!(count, super::BACKSTOP_MAX_ATTEMPTS);
    assert!(unbuildable_at.is_some());

    sqlx::query(
        "UPDATE memory.turn_facts_backstop_attempts
            SET retry_after = now() - interval '1 second' WHERE operator_turn_id = $1",
    )
    .bind(turn)
    .execute(&handle.pool)
    .await?;
    assert!(candidates(&handle).await?.is_empty());
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn stamp_turn_facts_built_at_clears_backstop_attempt() -> anyhow::Result<()> {
    let Some(db) = test_util::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (session, turn) = seed_closed_turn(&handle.pool, "/tmp/backstop-cleared").await?;
    assert_eq!(candidates(&handle).await?, vec![session]);

    handle
        .stamp_turn_facts_built_at(&[turn], Utc::now())
        .await?;

    assert_eq!(attempt_row(&handle.pool, turn).await?, None);
    assert_eq!(candidates(&handle).await?, vec![session]);
    db.cleanup().await?;
    Ok(())
}
