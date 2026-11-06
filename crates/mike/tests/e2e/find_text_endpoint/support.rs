use sqlx::PgPool;

pub(super) async fn ensure_parents(
    pool: &PgPool,
    session_id: &str,
    project_path: &str,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now();
    let project_id: i64 = sqlx::query_scalar(
        "WITH ins AS (
             INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
             SELECT $1, $1, $2, $2
             WHERE NOT EXISTS (
                 SELECT 1 FROM projects.projects WHERE project_path = $1
             )
             RETURNING id
         )
         SELECT id FROM ins
         UNION ALL
         SELECT id FROM projects.projects WHERE project_path = $1
         LIMIT 1",
    )
    .bind(project_path)
    .bind(now)
    .fetch_one(pool)
    .await?;

    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, $3, $3) ON CONFLICT (session_uuid) DO NOTHING",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(project_id)
}

pub(super) async fn seed_request_with_body(
    pool: &PgPool,
    session_id: &str,
    project_id: i64,
    body: &[u8],
    offset_ms: i64,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now() + chrono::Duration::milliseconds(offset_ms);
    let params_seed = serde_json::json!({"thinking_enabled": false});
    let request_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
            status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            request_body,
            created_at
         )
         SELECT
            (SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid),
            $2, rm.id, '/v1/messages', false, 0, false,
            rs.id, $3,
            '', '', rtt.id, rto.id,
            $5,
            $4
         FROM requests.methods rm, requests.statuses rs,
              requests.turn_types rtt, requests.turn_origins rto
         WHERE rm.code = 'POST' AND rs.code = 'complete'
           AND rtt.code = 'user_prompt' AND rto.code = 'primary'
         RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(&params_seed)
    .bind(now)
    .bind(body)
    .fetch_one(pool)
    .await?;

    Ok(request_id)
}

pub(super) struct SentPatchSeed<'a> {
    pub(super) session_id: &'a str,
    pub(super) project_id: i64,
    pub(super) request_body: &'a [u8],
    pub(super) patch_start: i32,
    pub(super) patch_end: i32,
    pub(super) patch_bytes: &'a [u8],
    pub(super) offset_ms: i64,
}

pub(super) async fn seed_request_with_sent_patch(
    pool: &PgPool,
    seed: SentPatchSeed<'_>,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now() + chrono::Duration::milliseconds(seed.offset_ms);
    let params_seed = serde_json::json!({"thinking_enabled": false});
    let request_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
            status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            request_body,
            sent_patch_start, sent_patch_end, sent_patch_bytes,
            created_at
         )
         SELECT
            (SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid),
            $2, rm.id, '/v1/messages', false, 0, false,
            rs.id, $3,
            '', '', rtt.id, rto.id,
            $5,
            $6, $7, $8,
            $4
         FROM requests.methods rm, requests.statuses rs,
              requests.turn_types rtt, requests.turn_origins rto
         WHERE rm.code = 'POST' AND rs.code = 'complete'
           AND rtt.code = 'user_prompt' AND rto.code = 'primary'
         RETURNING id",
    )
    .bind(seed.session_id)
    .bind(seed.project_id)
    .bind(&params_seed)
    .bind(now)
    .bind(seed.request_body)
    .bind(seed.patch_start)
    .bind(seed.patch_end)
    .bind(seed.patch_bytes)
    .fetch_one(pool)
    .await?;

    Ok(request_id)
}
