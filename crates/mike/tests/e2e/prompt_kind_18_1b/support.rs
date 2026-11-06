use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

pub(crate) async fn bootstrap_project_session(
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

pub(crate) struct InsertRequestParams<'a> {
    pub(crate) session_id: &'a str,
    pub(crate) project_id: i64,
    pub(crate) turn_origin: &'a str,
    pub(crate) turn_type: &'a str,
    pub(crate) status: &'a str,
    pub(crate) human_prompt: &'a str,
    pub(crate) completed: bool,
    pub(crate) created_at_offset_ms: i64,
}

pub(crate) async fn insert_request(
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

pub(crate) async fn insert_response(
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

pub(crate) async fn insert_text_chunk(
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

pub(crate) struct InsertTurnParams<'a> {
    pub(crate) session_id: &'a str,
    pub(crate) project_id: i64,
    pub(crate) start_request_id: i64,
    pub(crate) end_request_id: i64,
    pub(crate) user_prompt: Option<&'a str>,
    pub(crate) outcome: &'a str,
    pub(crate) closed: bool,
}

pub(crate) async fn insert_turn(pool: &PgPool, p: InsertTurnParams<'_>) -> anyhow::Result<i64> {
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

pub(crate) async fn insert_classification(
    pool: &PgPool,
    request_id: i64,
    session_id: &str,
    kind_code: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO prompt_kinds.classifications (request_id, session_id, kind_id)
         SELECT $1,
                (SELECT id FROM sessions.sessions WHERE session_uuid = $2::text::uuid),
                pk.id
           FROM prompt_kinds.kinds pk
          WHERE pk.code = $3
         ON CONFLICT (request_id) DO NOTHING",
    )
    .bind(request_id)
    .bind(session_id)
    .bind(kind_code)
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) fn minimal_sse_response() -> String {
    let mut body = String::new();
    body.push_str("event: message_start\ndata: ");
    body.push_str(
        &json!({
            "type": "message_start",
            "message": {
                "id": "msg_pk_test_01",
                "type": "message",
                "role": "assistant",
                "content": [],
                "model": "claude-sonnet-4-20250514",
                "stop_reason": null,
                "usage": {"input_tokens": 5, "output_tokens": 0}
            }
        })
        .to_string(),
    );
    body.push_str("\n\nevent: content_block_start\ndata: ");
    body.push_str(
        &json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}})
            .to_string(),
    );
    body.push_str("\n\nevent: content_block_delta\ndata: ");
    body.push_str(
        &json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"done"}})
            .to_string(),
    );
    body.push_str("\n\nevent: content_block_stop\ndata: ");
    body.push_str(&json!({"type":"content_block_stop","index":0}).to_string());
    body.push_str("\n\nevent: message_delta\ndata: ");
    body.push_str(
        &json!({
            "type": "message_delta",
            "delta": {"stop_reason": "end_turn", "stop_sequence": null},
            "usage": {"output_tokens": 1}
        })
        .to_string(),
    );
    body.push_str("\n\nevent: message_stop\ndata: ");
    body.push_str(&json!({"type":"message_stop"}).to_string());
    body.push_str("\n\n");
    body
}

pub(crate) async fn wait_for_prompt_kind_classification(
    pool: &PgPool,
    session_id: &str,
) -> anyhow::Result<String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT pk.code
               FROM prompt_kinds.classifications pkc
               JOIN prompt_kinds.kinds pk ON pk.id = pkc.kind_id
               JOIN requests.requests r ON r.id = pkc.request_id
              WHERE r.session_id = (
                  SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid
              )",
        )
        .bind(session_id)
        .fetch_optional(pool)
        .await?;
        if let Some((code,)) = row {
            return Ok(code);
        }
        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "timed out waiting for prompt_kind classification row"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}
