use serde_json::Value;
use sqlx::PgPool;

pub(super) async fn seed_project(pool: &PgPool, project_path: &str) -> anyhow::Result<i64> {
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

pub(super) async fn seed_session(
    pool: &PgPool,
    session_uuid: uuid::Uuid,
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

pub(super) async fn seed_streaming_request(
    pool: &PgPool,
    session_id_fk: i64,
    project_id: i64,
    model: &str,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, model, is_streaming,
            message_count, is_tool_loop, status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at
         )
         SELECT $1, $2, rm.id, '/v1/messages', $3, true,
                0, false, rs.id, '{\"thinking_enabled\": false}'::jsonb,
                '', '', rtt.id, rto.id, NOW()
         FROM requests.methods rm, requests.statuses rs,
              requests.turn_types rtt, requests.turn_origins rto
         WHERE rm.code = 'POST' AND rs.code = 'complete'
           AND rtt.code = 'user_prompt' AND rto.code = 'primary'
         RETURNING id",
    )
    .bind(session_id_fk)
    .bind(project_id)
    .bind(model)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(super) struct SeedBlock<'a> {
    pub(super) request_id: i64,
    pub(super) session_id_fk: i64,
    pub(super) seq_ord: i32,
    pub(super) content: &'a str,
    pub(super) block_code: &'a str,
    pub(super) signature: Option<&'a str>,
    pub(super) wire_tool_use_id: Option<&'a str>,
}

pub(super) async fn seed_block(pool: &PgPool, b: SeedBlock<'_>) -> anyhow::Result<()> {
    let sha = "a".repeat(64);
    sqlx::query(
        "INSERT INTO responses.stream_blocks
            (request_id, session_id, block_type_id, seq_ord, content,
             signature, wire_tool_use_id, content_sha256, byte_size)
         SELECT $1, $2,
                (SELECT id FROM responses.stream_block_types WHERE code = $3),
                $4, $5, $6, $7, $8, $9",
    )
    .bind(b.request_id)
    .bind(b.session_id_fk)
    .bind(b.block_code)
    .bind(b.seq_ord)
    .bind(b.content)
    .bind(b.signature)
    .bind(b.wire_tool_use_id)
    .bind(&sha)
    .bind(b.content.len() as i32)
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) async fn seed_tool_use(
    pool: &PgPool,
    request_id: i64,
    wire_id: &str,
    ord: i32,
    tool_name: &str,
    input: &Value,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO tools.uses
            (wire_tool_use_id, request_id, ord, tool_name, input, session_id)
         SELECT $1, $2, $3, $4, $5, r.session_id
         FROM requests.requests r WHERE r.id = $2",
    )
    .bind(wire_id)
    .bind(request_id)
    .bind(ord)
    .bind(tool_name)
    .bind(input)
    .execute(pool)
    .await?;
    Ok(())
}
