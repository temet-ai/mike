use chrono::{DateTime, Utc};
use mike::store::persistence::proxy_repo::ProxyPersist as _;
use uuid::Uuid;

use mike::store::test_support::seed_project;

use crate::store::remote_control::support::{shared_handle, unique};

async fn seed_session(
    pool: &sqlx::PgPool,
    project_id: i64,
    last_seen: DateTime<Utc>,
) -> anyhow::Result<Uuid> {
    let session_uuid = Uuid::new_v4();
    sqlx::query!(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, $3, $3)",
        session_uuid,
        project_id,
        last_seen,
    )
    .execute(pool)
    .await?;
    Ok(session_uuid)
}

async fn seed_request(
    pool: &sqlx::PgPool,
    session_id: i64,
    project_id: i64,
    request_body: Option<&[u8]>,
    age_seconds: i64,
) -> anyhow::Result<()> {
    sqlx::query!(
        "INSERT INTO requests.requests (
             session_id, project_id, method_id, path, is_streaming,
             message_count, is_tool_loop, status_id,
             params, last_user_message, human_prompt, turn_type_id,
             turn_origin_id, request_body, created_at, completed_at
         )
         SELECT $1, $2, rm.id, '/v1/messages', false,
                1, false, rs.id, '{}'::jsonb, '', '', rtt.id, rto.id,
                $3, NOW() + ($4 || ' seconds')::interval,
                NOW() + ($4 || ' seconds')::interval
           FROM requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto
          WHERE rm.code = 'POST' AND rs.code = 'complete'
            AND rtt.code = 'user_prompt' AND rto.code = 'primary'",
        session_id,
        project_id,
        request_body,
        age_seconds.to_string(),
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn session_bigint_id(pool: &sqlx::PgPool, session_uuid: Uuid) -> anyhow::Result<i64> {
    let id = sqlx::query_scalar!(
        "SELECT id FROM sessions.sessions WHERE session_uuid = $1",
        session_uuid,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

#[tokio::test]
async fn candidate_carries_its_newest_requests_body_not_an_older_one() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };

    let project_id = seed_project(handle.pool(), &unique("/tmp/link-sweep-newest")).await?;
    let now = Utc::now();
    let session_uuid = seed_session(handle.pool(), project_id, now).await?;
    let session_id = session_bigint_id(handle.pool(), session_uuid).await?;

    seed_request(
        handle.pool(),
        session_id,
        project_id,
        Some(b"older, no marker here"),
        -60,
    )
    .await?;
    seed_request(
        handle.pool(),
        session_id,
        project_id,
        Some(b"newer claude.ai/code/session_abc123"),
        -1,
    )
    .await?;

    let candidates = handle
        .proxy()
        .fetch_channel_link_sweep_candidates(now - chrono::Duration::hours(2), 200)
        .await?;
    let found = candidates
        .iter()
        .find(|c| c.session_uuid == session_uuid)
        .ok_or_else(|| anyhow::anyhow!("session must be a candidate"))?;
    assert_eq!(
        found.request_body.as_deref(),
        Some(&b"newer claude.ai/code/session_abc123"[..]),
        "must carry the NEWEST request's body, not an older one"
    );

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn a_session_with_a_linked_channel_is_never_a_candidate() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };

    let project_id = seed_project(handle.pool(), &unique("/tmp/link-sweep-linked")).await?;
    let now = Utc::now();
    let session_uuid = seed_session(handle.pool(), project_id, now).await?;
    let session_id = session_bigint_id(handle.pool(), session_uuid).await?;
    seed_request(handle.pool(), session_id, project_id, Some(b"body"), -1).await?;

    sqlx::query!(
        "INSERT INTO remote_control.channels (cse_id, local_session_uuid)
         VALUES ($1, $2)",
        unique("cse_already_linked"),
        session_uuid,
    )
    .execute(handle.pool())
    .await?;

    let candidates = handle
        .proxy()
        .fetch_channel_link_sweep_candidates(now - chrono::Duration::hours(2), 200)
        .await?;
    assert!(
        !candidates.iter().any(|c| c.session_uuid == session_uuid),
        "an already-linked session must never be selected"
    );

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn a_session_last_seen_outside_the_window_is_not_a_candidate() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };

    let project_id = seed_project(handle.pool(), &unique("/tmp/link-sweep-stale")).await?;
    let now = Utc::now();
    let stale_session =
        seed_session(handle.pool(), project_id, now - chrono::Duration::hours(5)).await?;
    let session_id = session_bigint_id(handle.pool(), stale_session).await?;
    seed_request(handle.pool(), session_id, project_id, Some(b"body"), -1).await?;

    let candidates = handle
        .proxy()
        .fetch_channel_link_sweep_candidates(now - chrono::Duration::hours(2), 200)
        .await?;
    assert!(
        !candidates.iter().any(|c| c.session_uuid == stale_session),
        "a session outside the recency window must not be swept forever"
    );

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn the_limit_bounds_the_candidate_count() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };

    let project_id = seed_project(handle.pool(), &unique("/tmp/link-sweep-limit")).await?;
    let now = Utc::now();
    for i in 0..5 {
        let session_uuid = seed_session(handle.pool(), project_id, now).await?;
        let session_id = session_bigint_id(handle.pool(), session_uuid).await?;
        seed_request(handle.pool(), session_id, project_id, Some(b"body"), -1 - i).await?;
    }

    let candidates = handle
        .proxy()
        .fetch_channel_link_sweep_candidates(now - chrono::Duration::hours(2), 3)
        .await?;
    assert_eq!(candidates.len(), 3, "LIMIT must bound the returned rows");

    handle.pool().close().await;
    Ok(())
}
