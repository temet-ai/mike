use sqlx::PgPool;

pub(super) async fn seed_request_row(
    pool: &PgPool,
    project_id: i64,
    session_id: &str,
    created_at: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<i64> {
    let session_uuid: uuid::Uuid = session_id.parse()?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming,
            message_count, is_tool_loop, status_id, params,
            last_user_message, human_prompt,
            turn_type_id, turn_origin_id, created_at
         )
         SELECT
                (SELECT id FROM sessions.sessions WHERE session_uuid = $1),
                $2, rm.id, '/v1/messages', false,
                0, false, rs.id, '{}'::jsonb,
                '', '',
                rtt.id, rto.id, $3
         FROM requests.methods rm, requests.statuses rs,
              requests.turn_types rtt, requests.turn_origins rto
         WHERE rm.code = 'POST' AND rs.code = 'complete'
           AND rtt.code = 'user_prompt' AND rto.code = 'primary'
         RETURNING id",
    )
    .bind(session_uuid)
    .bind(project_id)
    .bind(created_at)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(super) async fn seed_message(
    pool: &PgPool,
    sha: &str,
    role: &str,
    body: serde_json::Value,
) -> anyhow::Result<i64> {
    let byte_size = body.to_string().len() as i32;
    let id: i64 = sqlx::query_scalar(
        "WITH ins AS (
             INSERT INTO messages.messages (message_sha256, body, byte_size, role_id)
             SELECT $1, $2, $3, r.id
             FROM messages.roles r
             WHERE r.code = $4
               AND NOT EXISTS (
                   SELECT 1 FROM messages.messages WHERE message_sha256 = $1
               )
             RETURNING id
         )
         SELECT id FROM ins
         UNION ALL
         SELECT id FROM messages.messages WHERE message_sha256 = $1
         LIMIT 1",
    )
    .bind(sha)
    .bind(body)
    .bind(byte_size)
    .bind(role)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(super) struct SeedDeltaRowParams<'a> {
    pub(super) request_id: i64,
    pub(super) predecessor_request_id: Option<i64>,
    pub(super) session_id: &'a str,
    pub(super) change_kind: &'a str,
    pub(super) ord: i32,
    pub(super) message_id: i64,
    pub(super) message_sha256: &'a str,
    pub(super) removed_reason: Option<&'a str>,
}

pub(super) async fn seed_delta_row(pool: &PgPool, p: SeedDeltaRowParams<'_>) -> anyhow::Result<()> {
    let _ = p.message_sha256;
    let session_uuid: uuid::Uuid = p.session_id.parse()?;
    sqlx::query(
        "INSERT INTO requests.deltas (
            request_id, predecessor_request_id,
            session_id,
            change_kind_id, ord,
            message_id,
            removed_reason_id
         )
         SELECT $1, $2,
                (SELECT id FROM sessions.sessions WHERE session_uuid = $3),
                ck.id, $5,
                $6,
                rr.id
         FROM requests.delta_change_kinds ck
         LEFT JOIN requests.delta_removed_reasons rr ON rr.code = $7
         WHERE ck.code = $4",
    )
    .bind(p.request_id)
    .bind(p.predecessor_request_id)
    .bind(session_uuid)
    .bind(p.change_kind)
    .bind(p.ord)
    .bind(p.message_id)
    .bind(p.removed_reason)
    .execute(pool)
    .await?;
    Ok(())
}
