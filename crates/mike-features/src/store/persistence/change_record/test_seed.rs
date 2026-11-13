use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::change_record::{ChangeKind, NewChangeEvent};
use crate::store::persistence::test_support::seed_project;

pub const CHECKOUT: &str = "/work/alpha";

pub fn micros_now() -> anyhow::Result<DateTime<Utc>> {
    DateTime::from_timestamp_micros(Utc::now().timestamp_micros())
        .ok_or_else(|| anyhow::anyhow!("clock out of range"))
}

pub async fn project(pool: &PgPool) -> anyhow::Result<i64> {
    seed_project(pool, CHECKOUT).await
}

pub async fn session(
    pool: &PgPool,
    project_id: i64,
    cwd: Option<&str>,
) -> anyhow::Result<i64> {
    let id = sqlx::query_scalar!(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen, cwd)
         VALUES ($1, $2, NOW(), NOW(), $3)
         RETURNING id",
        Uuid::new_v4(),
        project_id,
        cwd,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub fn event(project_id: i64, kind: ChangeKind, paths: &[&str]) -> NewChangeEvent {
    NewChangeEvent {
        project_id,
        checkout: CHECKOUT.to_owned(),
        at: Utc::now(),
        kind,
        session_id: None,
        run_id: None,
        task: None,
        paths: paths.iter().map(|p| (*p).to_owned()).collect(),
        subject: format!("{} {}", kind.as_str(), paths.join(",")),
        detail: serde_json::json!({}),
        before_sha: None,
        after_sha: None,
    }
}

pub async fn request(
    pool: &PgPool,
    session_id: i64,
    project_id: i64,
    at: DateTime<Utc>,
) -> anyhow::Result<i64> {
    let id = sqlx::query_scalar!(
        "INSERT INTO requests.requests (
             session_id, project_id, method_id, path, is_streaming,
             message_count, is_tool_loop, status_id,
             params, last_user_message, human_prompt, turn_type_id, turn_origin_id,
             created_at, completed_at
         )
         SELECT $1, $2, rm.id, '/v1/messages', false,
                0, false, rs.id,
                '{}'::jsonb, '', '', rtt.id, rto.id,
                $3, $3
           FROM requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto
          WHERE rm.code = 'POST' AND rs.code = 'complete'
            AND rtt.code = 'user_prompt' AND rto.code = 'primary'
         RETURNING id",
        session_id,
        project_id,
        at,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub async fn response(
    pool: &PgPool,
    request_id: i64,
    completed_at: DateTime<Utc>,
) -> anyhow::Result<()> {
    sqlx::query!(
        "INSERT INTO responses.responses (request_id, response_status, token_usage, completed_at)
         VALUES ($1, 200, '{}'::jsonb, $2)",
        request_id,
        completed_at,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub struct ToolUse<'a> {
    pub session_id: i64,
    pub request_id: i64,
    pub name: &'a str,
    pub file_path: Option<&'a str>,
    pub at: DateTime<Utc>,
}

pub async fn tool_use(pool: &PgPool, use_: &ToolUse<'_>) -> anyhow::Result<i64> {
    let id = sqlx::query_scalar!(
        "INSERT INTO tools.uses (wire_tool_use_id, request_id, session_id, ord, tool_name, file_path, created_at)
         SELECT $1, $2, $3, count(*)::int, $4, $5, $6 FROM tools.uses WHERE request_id = $2
         RETURNING id",
        format!("toolu_{}", Uuid::new_v4()),
        use_.request_id,
        use_.session_id,
        use_.name,
        use_.file_path,
        use_.at,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub async fn tool_use_with_input(
    pool: &PgPool,
    use_: &ToolUse<'_>,
    input: &serde_json::Value,
) -> anyhow::Result<i64> {
    let id = tool_use(pool, use_).await?;
    sqlx::query!("UPDATE tools.uses SET input = $2 WHERE id = $1", id, input)
        .execute(pool)
        .await?;
    Ok(id)
}

pub async fn tool_result(
    pool: &PgPool,
    tool_use_id: i64,
    request_id: i64,
    session_id: i64,
    at: DateTime<Utc>,
) -> anyhow::Result<()> {
    sqlx::query!(
        "INSERT INTO tools.results (request_id, tool_use_id, ord, is_error, content, content_sha256, content_bytes, created_at, session_id)
         VALUES ($1, $2, 0, false, '\"ok\"'::jsonb, repeat('0', 64), 4, $3, $4)",
        request_id,
        tool_use_id,
        at,
        session_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn failed_tool_result(
    pool: &PgPool,
    tool_use_id: i64,
    request_id: i64,
    session_id: i64,
    at: DateTime<Utc>,
) -> anyhow::Result<()> {
    tool_result(pool, tool_use_id, request_id, session_id, at).await?;
    sqlx::query!(
        "UPDATE tools.results SET is_error = true WHERE tool_use_id = $1",
        tool_use_id
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub struct Fixture {
    pub db: crate::store::persistence::test_support::TestDbHandle,
    pub handle: crate::store::persistence::PersistHandle,
}

pub async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(db) = crate::store::persistence::test_support::acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(None);
    };
    let handle = crate::store::persistence::PersistHandle::connect(&db.database_url).await?;
    Ok(Some(Fixture { db, handle }))
}
