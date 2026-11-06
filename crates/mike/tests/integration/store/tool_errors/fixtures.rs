pub async fn seed_project(pool: &sqlx::PgPool, project_path: &str) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar!(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
             VALUES ($1, $1, NOW(), NOW()) RETURNING id",
        project_path,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub async fn seed_session(
    pool: &sqlx::PgPool,
    session_uuid: &str,
    project_id: i64,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar!(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
             VALUES ($1::text::uuid, $2, NOW(), NOW()) RETURNING id",
        session_uuid,
        project_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub async fn seed_request(
    pool: &sqlx::PgPool,
    session_id: i64,
    project_id: i64,
) -> anyhow::Result<i64> {
    let request_id: i64 = sqlx::query_scalar!(
        "INSERT INTO requests.requests (
                        session_id, project_id, method_id, path, is_streaming, message_count,
                        is_tool_loop, status_id, params, last_user_message, human_prompt,
                        turn_type_id, turn_origin_id, created_at
                     )
                     SELECT $1, $2, rm.id, '/v1/messages', false, 0, false,
                            rs.id, '{}'::jsonb, '', '', rtt.id, rto.id, NOW()
                     FROM requests.methods rm, requests.statuses rs,
                          requests.turn_types rtt, requests.turn_origins rto
                     WHERE rm.code = 'POST' AND rs.code = 'complete'
                       AND rtt.code = 'user_prompt' AND rto.code = 'primary'
                     RETURNING id",
        session_id,
        project_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(request_id)
}
