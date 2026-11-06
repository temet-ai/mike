use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

pub(super) async fn bootstrap_project_session(
    pool: &PgPool,
    project_path: &str,
    session_id: &str,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now();
    sqlx::query(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         SELECT $1, $1, $2, $2
         WHERE NOT EXISTS (
             SELECT 1 FROM projects.projects WHERE project_path = $1
         )",
    )
    .bind(project_path)
    .bind(now)
    .execute(pool)
    .await?;
    let project_id: i64 = sqlx::query_scalar(
        "SELECT id FROM projects.projects WHERE project_path = $1 ORDER BY id LIMIT 1",
    )
    .bind(project_path)
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

pub(super) struct InsertRequestParams<'a> {
    pub(super) session_id: &'a str,
    pub(super) project_id: i64,
    pub(super) turn_origin: &'a str,
    pub(super) turn_type: &'a str,
    pub(super) status: &'a str,
    pub(super) human_prompt: &'a str,
    pub(super) completed: bool,
    pub(super) created_at_offset_ms: i64,
}

pub(super) async fn insert_request(
    pool: &PgPool,
    p: InsertRequestParams<'_>,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now() + chrono::Duration::milliseconds(p.created_at_offset_ms);
    let completed_at = p.completed.then_some(now);
    let empty_obj = json!({});
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
            status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at, completed_at
         )
         SELECT (SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid),
                $2, rm.id, '/v1/messages', false, 0, false,
                rs.id, $3,
                '', $4, rtt.id, rto.id,
                $7, $8
         FROM requests.methods rm, requests.statuses rs,
              requests.turn_types rtt, requests.turn_origins rto
         WHERE rm.code = 'POST' AND rs.code = $9
           AND rtt.code = $5 AND rto.code = $6
         RETURNING id",
    )
    .bind(p.session_id)
    .bind(p.project_id)
    .bind(&empty_obj)
    .bind(p.human_prompt)
    .bind(p.turn_type)
    .bind(p.turn_origin)
    .bind(now)
    .bind(completed_at)
    .bind(p.status)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(super) async fn insert_response(
    pool: &PgPool,
    request_id: i64,
    stop_reason: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO responses.responses (request_id, response_status, stop_reason_id, completed_at)
         SELECT $1, 200, sr.id, NOW()
         FROM responses.stop_reasons sr
         WHERE sr.code = $2",
    )
    .bind(request_id)
    .bind(stop_reason)
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) async fn insert_text_chunk(
    pool: &PgPool,
    request_id: i64,
    session_id: &str,
    seq_ord: i32,
    content: &str,
) -> anyhow::Result<()> {
    let sha = {
        let mut h = Sha256::new();
        h.update(content.as_bytes());
        hex::encode(h.finalize())
    };
    sqlx::query(
        "INSERT INTO responses.stream_blocks (
            request_id, session_id, seq_ord, content,
            byte_size, content_sha256, block_type_id
         )
         SELECT $1, (SELECT id FROM sessions.sessions WHERE session_uuid = $2::text::uuid),
                $3, $4, octet_length($4), $5, bt.id
         FROM responses.stream_block_types bt
         WHERE bt.code = 'text'",
    )
    .bind(request_id)
    .bind(session_id)
    .bind(seq_ord)
    .bind(content)
    .bind(sha)
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) struct InsertTurnParams<'a> {
    pub(super) session_id: &'a str,
    pub(super) project_id: i64,
    pub(super) start_request_id: i64,
    pub(super) end_request_id: i64,
    pub(super) user_prompt: Option<&'a str>,
    pub(super) outcome: &'a str,
    pub(super) closed: bool,
}

pub(super) async fn insert_turn(pool: &PgPool, p: InsertTurnParams<'_>) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turns (
            session_id, project_id,
            start_request_id, end_request_id,
            user_prompt, outcome_id, classified_at, closed_at
         )
         SELECT (SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid), $2,
                $3, $4,
                $5, toc.id,
                CASE WHEN $7 THEN NOW() END,
                CASE WHEN $7 THEN NOW() END
         FROM memory.turn_outcomes toc
         WHERE toc.code = $6
         RETURNING id",
    )
    .bind(p.session_id)
    .bind(p.project_id)
    .bind(p.start_request_id)
    .bind(p.end_request_id)
    .bind(p.user_prompt)
    .bind(p.outcome)
    .bind(p.closed)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(super) fn turn_by_id(overview: &Value, turn_id: i64) -> anyhow::Result<&Value> {
    overview["turns"]
        .as_array()
        .and_then(|a| a.iter().find(|t| t["turn_id"] == turn_id))
        .ok_or_else(|| anyhow::anyhow!("turn {turn_id} not in overview: {overview}"))
}
