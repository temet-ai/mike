use sqlx::PgPool;

pub(super) struct ToolCallSeed<'a> {
    pub project_path: &'a str,
    pub session_id: &'a str,
    pub tool_name: &'a str,
    pub operation: &'a str,
    pub file_path: &'a str,
    pub wire_id: &'a str,
    pub offset_ms: i64,
}

pub(super) async fn insert_tool_call(pool: &PgPool, seed: ToolCallSeed<'_>) -> anyhow::Result<()> {
    let now = chrono::Utc::now() + chrono::Duration::milliseconds(seed.offset_ms);

    sqlx::query(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         SELECT $1, $1, $2, $2
          WHERE NOT EXISTS (SELECT 1 FROM projects.projects WHERE project_path = $1)",
    )
    .bind(seed.project_path)
    .bind(now)
    .execute(pool)
    .await?;
    let project_id: i64 = sqlx::query_scalar(
        "SELECT id FROM projects.projects WHERE project_path = $1 ORDER BY id LIMIT 1",
    )
    .bind(seed.project_path)
    .fetch_one(pool)
    .await?;

    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, $3, $3) ON CONFLICT (session_uuid) DO NOTHING",
    )
    .bind(seed.session_id)
    .bind(project_id)
    .bind(now)
    .execute(pool)
    .await?;

    let (request_id, session_db_id): (i64, i64) = sqlx::query_as(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
            status_id, params, last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at
         )
         SELECT s.id, $2, rm.id, '/v1/messages', false, 0, false,
                rs.id, '{}'::jsonb, '', '', rtt.id, rto.id, $3
           FROM sessions.sessions s,
                requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto
          WHERE s.session_uuid = $1::text::uuid
            AND rm.code = 'POST' AND rs.code = 'complete'
            AND rtt.code = 'user_prompt' AND rto.code = 'primary'
         RETURNING id, session_id",
    )
    .bind(seed.session_id)
    .bind(project_id)
    .bind(now)
    .fetch_one(pool)
    .await?;

    sqlx::query(
        "INSERT INTO tools.uses
            (wire_tool_use_id, request_id, ord, tool_name, input, file_path, operation,
             session_id, created_at)
         VALUES ($1, $2, 1, $3, '{}'::jsonb, $4, $5, $6, $7)",
    )
    .bind(seed.wire_id)
    .bind(request_id)
    .bind(seed.tool_name)
    .bind(seed.file_path)
    .bind(seed.operation)
    .bind(session_db_id)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(())
}

pub(super) struct EditSeed<'a> {
    pub project_path: &'a str,
    pub session_id: &'a str,
    pub file_path: &'a str,
    pub wire_id: &'a str,
    pub old_string: &'a str,
    pub new_string: &'a str,
    pub offset_ms: i64,
}

pub(super) async fn insert_edit(pool: &PgPool, seed: EditSeed<'_>) -> anyhow::Result<()> {
    insert_tool_call(
        pool,
        ToolCallSeed {
            project_path: seed.project_path,
            session_id: seed.session_id,
            tool_name: "Edit",
            operation: "write",
            file_path: seed.file_path,
            wire_id: seed.wire_id,
            offset_ms: seed.offset_ms,
        },
    )
    .await?;
    sqlx::query(
        "UPDATE tools.uses
            SET input = jsonb_build_object(
                  'file_path',  $2::text,
                  'old_string', $3::text,
                  'new_string', $4::text)
          WHERE wire_tool_use_id = $1",
    )
    .bind(seed.wire_id)
    .bind(seed.file_path)
    .bind(seed.old_string)
    .bind(seed.new_string)
    .execute(pool)
    .await?;
    Ok(())
}
