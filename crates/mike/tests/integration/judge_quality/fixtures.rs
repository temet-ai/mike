use chrono::DateTime;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

pub(super) async fn bootstrap_project_session(
    pool: &PgPool,
    project_path: &str,
) -> anyhow::Result<(i64, i64, String)> {
    let session_uuid = Uuid::new_v4().to_string();
    let now = Utc::now();

    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, $2, $2)
         ON CONFLICT (project_path) DO UPDATE SET last_seen = EXCLUDED.last_seen
         RETURNING id",
    )
    .bind(project_path)
    .bind(now)
    .fetch_one(pool)
    .await?;

    let session_pk: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, $3, $3)
         ON CONFLICT (session_uuid) DO UPDATE SET last_seen = EXCLUDED.last_seen
         RETURNING id",
    )
    .bind(&session_uuid)
    .bind(project_id)
    .bind(now)
    .fetch_one(pool)
    .await?;

    Ok((project_id, session_pk, session_uuid))
}

pub(super) async fn resolve_outcome_id(pool: &PgPool, code: &str) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar("SELECT id FROM session_judges.outcomes WHERE code = $1")
        .bind(code)
        .fetch_one(pool)
        .await?;
    Ok(id)
}

pub(super) async fn seed_completed_request(
    pool: &PgPool,
    session_pk: i64,
    project_id: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO requests.requests
             (session_id, project_id, method_id, path, is_streaming, message_count,
              is_tool_loop, status_id, params, last_user_message, human_prompt,
              turn_type_id, turn_origin_id, created_at)
         VALUES (
             $1, $2,
             (SELECT id FROM requests.methods WHERE code = 'POST'),
             '/v1/messages', false, 1, false,
             (SELECT id FROM requests.statuses WHERE code = 'complete'),
             '{}'::jsonb, 'hello', '',
             (SELECT id FROM requests.turn_types WHERE code = 'user_prompt'),
             (SELECT id FROM requests.turn_origins WHERE code = 'primary'),
             NOW() - interval '10 minutes'
         )",
    )
    .bind(session_pk)
    .bind(project_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) async fn seed_session_with_last_seen(
    pool: &PgPool,
    project_path: &str,
    last_seen: DateTime<Utc>,
) -> anyhow::Result<(i64, String)> {
    let (project_id, session_pk, session_uuid) =
        bootstrap_project_session(pool, project_path).await?;
    seed_unjudged_operator_turn(pool, session_pk, project_id, last_seen).await?;
    sqlx::query("UPDATE sessions.sessions SET last_seen = $1 WHERE id = $2")
        .bind(last_seen)
        .bind(session_pk)
        .execute(pool)
        .await?;
    Ok((session_pk, session_uuid))
}

pub(super) async fn seed_unjudged_operator_turn(
    pool: &PgPool,
    session_pk: i64,
    project_id: i64,
    closed_at: DateTime<Utc>,
) -> anyhow::Result<i64> {
    let request_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests
             (session_id, project_id, method_id, path, is_streaming, message_count,
              is_tool_loop, status_id, params, last_user_message, human_prompt,
              turn_type_id, turn_origin_id, created_at)
         VALUES (
             $1, $2,
             (SELECT id FROM requests.methods WHERE code = 'POST'),
             '/v1/messages', false, 1, false,
             (SELECT id FROM requests.statuses WHERE code = 'complete'),
             '{}'::jsonb, 'fix the bug', '',
             (SELECT id FROM requests.turn_types WHERE code = 'user_prompt'),
             (SELECT id FROM requests.turn_origins WHERE code = 'primary'),
             $3
         )
         RETURNING id",
    )
    .bind(session_pk)
    .bind(project_id)
    .bind(closed_at)
    .fetch_one(pool)
    .await?;

    let turn_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turns
             (session_id, project_id, start_request_id, end_request_id,
              outcome_id, closed_at, user_prompt, final_assistant_text)
         VALUES ($1, $2, $3, $3, 1, $4, 'fix the bug', 'I fixed the bug.')
         RETURNING id",
    )
    .bind(session_pk)
    .bind(project_id)
    .bind(request_id)
    .bind(closed_at)
    .fetch_one(pool)
    .await?;

    Ok(turn_id)
}
