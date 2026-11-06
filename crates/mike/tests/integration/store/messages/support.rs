use chrono::{DateTime, Utc};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use mike::store::persistence::test_support;

pub(super) async fn test_pool() -> anyhow::Result<Option<(PgPool, test_support::TestDbHandle)>> {
    let Some(factory) = test_support::TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let handle_db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;
    Ok(Some((pool, handle_db)))
}

pub(super) async fn test_pool_isolated()
-> anyhow::Result<Option<(PgPool, test_support::TestDbHandle)>> {
    let Some(factory) = test_support::TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let handle_db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;
    Ok(Some((pool, handle_db)))
}

pub(super) async fn seed_project_session(
    pool: &PgPool,
    project_path: &str,
    session_id: &str,
) -> anyhow::Result<(i64, i64)> {
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects
                (project_path, display_name, first_seen, last_seen)
             VALUES ($1, $1, NOW(), NOW())
             RETURNING id",
    )
    .bind(project_path)
    .fetch_one(pool)
    .await?;
    let session_fk: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions
                (session_uuid, project_id, first_seen, last_seen)
             VALUES ($1::text::uuid, $2, NOW(), NOW())
             RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    Ok((project_id, session_fk))
}

pub(super) async fn seed_request(
    pool: &PgPool,
    session_fk: i64,
    project_id: i64,
    turn_origin: &str,
    created_at: DateTime<Utc>,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
                session_id, project_id, method_id, path, is_streaming,
                message_count, is_tool_loop, status_id, params,
                last_user_message, human_prompt, turn_type_id, turn_origin_id,
                created_at
             )
             SELECT $1, $2, rm.id, '/v1/messages', false,
                    0, false, rs.id, '{}'::jsonb, '', '', rtt.id, rto.id, $4
             FROM requests.methods rm, requests.statuses rs,
                  requests.turn_types rtt, requests.turn_origins rto
             WHERE rm.code = 'POST' AND rs.code = 'complete'
               AND rtt.code = 'user_prompt' AND rto.code = $3
             RETURNING id",
    )
    .bind(session_fk)
    .bind(project_id)
    .bind(turn_origin)
    .bind(created_at)
    .fetch_one(pool)
    .await?;
    Ok(id)
}
