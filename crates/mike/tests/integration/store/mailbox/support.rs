use chrono::{DateTime, Utc};

pub(super) async fn seed_session(pool: &sqlx::PgPool, label: &str) -> anyhow::Result<uuid::Uuid> {
    let project_id: i64 = sqlx::query_scalar!(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
             VALUES ($1, $1, NOW(), NOW()) RETURNING id",
        label,
    )
    .fetch_one(pool)
    .await?;
    let session_uuid = uuid::Uuid::new_v4();
    sqlx::query!(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
             VALUES ($1, $2, NOW(), NOW())",
        session_uuid,
        project_id,
    )
    .execute(pool)
    .await?;
    Ok(session_uuid)
}

pub(super) async fn seed_session_with_ids(
    pool: &sqlx::PgPool,
    label: &str,
) -> anyhow::Result<(uuid::Uuid, i64, i64)> {
    let project_id: i64 = sqlx::query_scalar!(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
             VALUES ($1, $1, NOW(), NOW()) RETURNING id",
        label,
    )
    .fetch_one(pool)
    .await?;
    let session_uuid = uuid::Uuid::new_v4();
    let session_id: i64 = sqlx::query_scalar!(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
             VALUES ($1, $2, NOW(), NOW()) RETURNING id",
        session_uuid,
        project_id,
    )
    .fetch_one(pool)
    .await?;
    Ok((session_uuid, session_id, project_id))
}

pub(super) async fn seed_request_at(
    pool: &sqlx::PgPool,
    session_id: i64,
    project_id: i64,
    turn_origin_code: &str,
    created_at: DateTime<Utc>,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar!(
        r#"INSERT INTO requests.requests
                   (session_id, project_id, method_id, path, is_streaming, message_count,
                    is_tool_loop, status_id, params, last_user_message, human_prompt,
                    turn_type_id, turn_origin_id, created_at)
               SELECT $1, $2, rm.id, '/v1/messages', false, 1, false, rs.id,
                      '{}'::jsonb, '', '', rtt.id, rto.id, $4
                 FROM requests.methods rm, requests.statuses rs,
                      requests.turn_types rtt, requests.turn_origins rto
                WHERE rm.code = 'POST' AND rs.code = 'complete'
                  AND rtt.code = 'user_prompt' AND rto.code = $3
               RETURNING id"#,
        session_id,
        project_id,
        turn_origin_code,
        created_at,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}
