use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use mike::store::test_support::TestDb;

pub async fn shared_pool() -> anyhow::Result<Option<PgPool>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = test_db.acquire_shared().await?;
    let pool = PgPool::connect(&db.database_url).await?;
    Ok(Some(pool))
}

pub async fn isolated_pool()
-> anyhow::Result<Option<(PgPool, mike::store::test_support::TestDbHandle)>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = test_db.acquire_isolated().await?;
    let pool = PgPool::connect(&db.database_url).await?;
    Ok(Some((pool, db)))
}

pub fn unique(label: &str) -> String {
    format!("{label}-{}", Uuid::new_v4().simple())
}

pub async fn seed_project(pool: &PgPool, project_path: &str) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects
            (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW())
         RETURNING id",
    )
    .bind(project_path)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub async fn seed_session(
    pool: &PgPool,
    session_uuid: Uuid,
    project_id: i64,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions
            (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, NOW(), NOW())
         RETURNING id",
    )
    .bind(session_uuid)
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub struct SeedRequest<'a> {
    pub session_id_fk: i64,
    pub project_id: i64,
    pub model: Option<&'a str>,
    pub turn_origin: &'a str,
    pub created_at: DateTime<Utc>,
    pub agent_instance_id: Option<i64>,
    pub human_prompt: &'a str,
}

pub async fn seed_request(pool: &PgPool, r: SeedRequest<'_>) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, model, is_streaming,
            message_count, is_tool_loop, status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at, agent_instance_id
         )
         SELECT $1, $2, rm.id, '/v1/messages', $3, false,
                0, false, rs.id, '{\"thinking_enabled\": false}'::jsonb,
                '', $7, rtt.id, rto.id, $5, $6
         FROM requests.methods rm, requests.statuses rs,
              requests.turn_types rtt, requests.turn_origins rto
         WHERE rm.code = 'POST' AND rs.code = 'complete'
           AND rtt.code = 'user_prompt' AND rto.code = $4
         RETURNING id",
    )
    .bind(r.session_id_fk)
    .bind(r.project_id)
    .bind(r.model)
    .bind(r.turn_origin)
    .bind(r.created_at)
    .bind(r.agent_instance_id)
    .bind(r.human_prompt)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub async fn seed_request_with_status(
    pool: &PgPool,
    session_id_fk: i64,
    project_id: i64,
    status_code: &str,
    completed_at: Option<DateTime<Utc>>,
    error_message: Option<&str>,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, model, is_streaming,
            message_count, is_tool_loop, status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at, completed_at
         )
         SELECT $1, $2, rm.id, '/v1/messages', NULL, true,
                0, false, rs.id, '{\"thinking_enabled\": false}'::jsonb,
                '', '', rtt.id, rto.id, NOW(), $4
         FROM requests.methods rm, requests.statuses rs,
              requests.turn_types rtt, requests.turn_origins rto
         WHERE rm.code = 'POST' AND rs.code = $3
           AND rtt.code = 'user_prompt' AND rto.code = 'primary'
         RETURNING id",
    )
    .bind(session_id_fk)
    .bind(project_id)
    .bind(status_code)
    .bind(completed_at)
    .fetch_one(pool)
    .await?;
    if let Some(msg) = error_message {
        sqlx::query!(
            "INSERT INTO requests.errors (request_id, message) VALUES ($1, $2)",
            id,
            msg,
        )
        .execute(pool)
        .await?;
    }
    Ok(id)
}

pub async fn seed_agent(pool: &PgPool) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar("INSERT INTO agents.agents (name) VALUES ($1) RETURNING id")
        .bind(format!("agent-{}", uuid::Uuid::new_v4()))
        .fetch_one(pool)
        .await?;
    Ok(id)
}

pub async fn seed_agent_instance(
    pool: &PgPool,
    session_fk: i64,
    dispatching_request_id: i64,
    agent_id: i64,
    ord: i32,
) -> anyhow::Result<i64> {
    let tool_use_id: i64 = sqlx::query_scalar(
        "INSERT INTO tools.uses (wire_tool_use_id, request_id, ord, tool_name, session_id)
         SELECT $1, $2, $3, 'Task', r.session_id
         FROM requests.requests r WHERE r.id = $2
         RETURNING id",
    )
    .bind(format!("tu-{}", uuid::Uuid::new_v4()))
    .bind(dispatching_request_id)
    .bind(ord)
    .fetch_one(pool)
    .await?;
    let dispatch_id: i64 = sqlx::query_scalar(
        "INSERT INTO agents.dispatches (tool_use_id, agent_id)
         VALUES ($1, $2) RETURNING id",
    )
    .bind(tool_use_id)
    .bind(agent_id)
    .fetch_one(pool)
    .await?;
    let instance_id: i64 = sqlx::query_scalar(
        "INSERT INTO agents.instances (session_id, dispatch_id, agent_id)
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(session_fk)
    .bind(dispatch_id)
    .bind(agent_id)
    .fetch_one(pool)
    .await?;
    Ok(instance_id)
}

pub async fn seed_scoped_request(
    pool: &PgPool,
    session_fk: i64,
    project_id: i64,
    turn_origin: &str,
    created_at: DateTime<Utc>,
    agent_instance_id: Option<i64>,
) -> anyhow::Result<i64> {
    seed_request(
        pool,
        SeedRequest {
            session_id_fk: session_fk,
            project_id,
            model: None,
            turn_origin,
            created_at,
            agent_instance_id,
            human_prompt: "",
        },
    )
    .await
}
