use chrono::DateTime;
use chrono::Utc;
use sqlx::PgPool;

pub(super) async fn seed_session(
    pool: &PgPool,
    session_id: &str,
    project_id: i64,
) -> anyhow::Result<i64> {
    let now = Utc::now();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions
            (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, $3, $3)
         ON CONFLICT (session_uuid) DO UPDATE SET last_seen = EXCLUDED.last_seen
         RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(super) async fn seed_tool_use(
    pool: &PgPool,
    tool_id: &str,
    request_id: i64,
    ord: i32,
    created_at: DateTime<Utc>,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO tools.uses
            (wire_tool_use_id, request_id, ord, tool_name, created_at, session_id)
         VALUES ($1, $2, $3, 'Agent', $4,
                 (SELECT session_id FROM requests.requests WHERE id = $2))
         ON CONFLICT (wire_tool_use_id) DO UPDATE SET wire_tool_use_id = EXCLUDED.wire_tool_use_id
         RETURNING id",
    )
    .bind(tool_id)
    .bind(request_id)
    .bind(ord)
    .bind(created_at)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(super) async fn seed_dispatch(pool: &PgPool, tool_use_pk: i64) -> anyhow::Result<i64> {
    let agent_id: i64 = sqlx::query_scalar(
        "WITH existing AS (
             SELECT id FROM agents.agents WHERE name = $1 LIMIT 1
         ), ins AS (
             INSERT INTO agents.agents (name)
             SELECT $1 WHERE NOT EXISTS (SELECT 1 FROM existing)
             RETURNING id
         )
         SELECT id FROM ins
         UNION ALL
         SELECT id FROM existing
         LIMIT 1",
    )
    .bind(format!("test-agent-{tool_use_pk}"))
    .fetch_one(pool)
    .await?;

    let dispatch_id: i64 = sqlx::query_scalar(
        "WITH ins AS (
             INSERT INTO agents.dispatches (tool_use_id, agent_id)
             SELECT $1, $2
             WHERE NOT EXISTS (
                 SELECT 1 FROM agents.dispatches WHERE tool_use_id = $1
             )
             RETURNING id
         )
         SELECT id FROM ins
         UNION ALL
         SELECT id FROM agents.dispatches WHERE tool_use_id = $1
         LIMIT 1",
    )
    .bind(tool_use_pk)
    .bind(agent_id)
    .fetch_one(pool)
    .await?;
    Ok(dispatch_id)
}

pub(super) async fn get_request_instance(
    pool: &PgPool,
    request_id: i64,
) -> anyhow::Result<Option<i64>> {
    let id: Option<Option<i64>> =
        sqlx::query_scalar("SELECT agent_instance_id FROM requests.requests WHERE id = $1")
            .bind(request_id)
            .fetch_optional(pool)
            .await?;
    Ok(id.flatten())
}

pub(super) async fn count_instances(pool: &PgPool, session_pk: i64) -> anyhow::Result<i64> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM agents.instances WHERE session_id = $1")
            .bind(session_pk)
            .fetch_one(pool)
            .await?;
    Ok(count)
}
