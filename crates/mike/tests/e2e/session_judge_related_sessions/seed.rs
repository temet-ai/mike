use chrono::Utc;
use sqlx::PgPool;

pub(super) async fn seed_project(pool: &PgPool, project_path: &str) -> anyhow::Result<i64> {
    let now = Utc::now();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, $2, $2)
         RETURNING id",
    )
    .bind(project_path)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(super) async fn seed_session(
    pool: &PgPool,
    project_id: i64,
    title: Option<&str>,
    last_seen: chrono::DateTime<Utc>,
) -> anyhow::Result<(i64, uuid::Uuid)> {
    let session_uuid = uuid::Uuid::new_v4();
    let now = Utc::now();
    let pk: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, title, first_seen, last_seen)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id",
    )
    .bind(session_uuid)
    .bind(project_id)
    .bind(title)
    .bind(now)
    .bind(last_seen)
    .fetch_one(pool)
    .await?;
    Ok((pk, session_uuid))
}

pub(super) async fn seed_closed_turn(
    pool: &PgPool,
    session_pk: i64,
    project_id: i64,
    user_prompt: &str,
    final_assistant_text: &str,
) -> anyhow::Result<()> {
    let now = Utc::now();
    let request_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
            status_id, params, last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at
         ) VALUES (
            $1, $2,
            (SELECT id FROM requests.methods WHERE code = 'POST'),
            '/v1/messages', false, 1, false,
            (SELECT id FROM requests.statuses WHERE code = 'complete'),
            '{}'::jsonb, $3, $3,
            (SELECT id FROM requests.turn_types WHERE code = 'user_prompt'),
            (SELECT id FROM requests.turn_origins WHERE code = 'primary'),
            $4
         ) RETURNING id",
    )
    .bind(session_pk)
    .bind(project_id)
    .bind(user_prompt)
    .bind(now)
    .fetch_one(pool)
    .await?;

    sqlx::query(
        "INSERT INTO memory.operator_turns
            (session_id, project_id, start_request_id, end_request_id, closed_at,
             user_prompt, final_assistant_text)
         VALUES ($1, $2, $3, $3, $4, $5, $6)",
    )
    .bind(session_pk)
    .bind(project_id)
    .bind(request_id)
    .bind(now)
    .bind(user_prompt)
    .bind(final_assistant_text)
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) fn planar_unit(angle_deg: f32) -> Vec<f32> {
    let mut v = vec![0.0_f32; 768];
    let rad = angle_deg.to_radians();
    v[0] = rad.cos();
    v[1] = rad.sin();
    v
}

pub(super) async fn count_lessons(pool: &PgPool) -> anyhow::Result<i64> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memory.lessons")
        .fetch_one(pool)
        .await?;
    Ok(count)
}
