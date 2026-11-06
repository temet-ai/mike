use crate::anthropic_fixtures;

use anthropic_fixtures::RequestBody;
use chrono::Utc;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::time::Duration;

async fn seed_project(pool: &PgPool, project_path: &str) -> anyhow::Result<i64> {
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

async fn seed_session(pool: &PgPool, project_id: i64, session_id: &str) -> anyhow::Result<()> {
    let now = Utc::now();
    sqlx::query(
        "INSERT INTO sessions.sessions
             (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, $3, $3)
         ON CONFLICT (session_uuid) DO NOTHING",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

async fn seed_request_row(
    pool: &PgPool,
    project_id: i64,
    id: i64,
    session_id: &str,
    created_at: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<()> {
    let empty_obj = serde_json::json!({});
    sqlx::query(
        "INSERT INTO requests.requests (
            id, session_id, project_id, method_id, path, is_streaming,
            message_count, is_tool_loop, status_id, params,
            last_user_message, human_prompt, turn_type_id,
            turn_origin_id, created_at
         )
         OVERRIDING SYSTEM VALUE
         VALUES (
            $1, (SELECT id FROM sessions.sessions WHERE session_uuid = $2::text::uuid),
            $3, 2, '/v1/messages', false,
            0, false, 3, $4,
            '', '', 1,
            1, $5
         )",
    )
    .bind(id)
    .bind(session_id)
    .bind(project_id)
    .bind(&empty_obj)
    .bind(created_at)
    .execute(pool)
    .await?;
    Ok(())
}

fn sha256_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex::encode(h.finalize())
}

async fn seed_block(
    pool: &PgPool,
    request_id: i64,
    session_id: &str,
    seq_ord: i32,
    content: &str,
    block_type: &str,
) -> anyhow::Result<()> {
    let byte_size = content.len() as i32;
    let content_sha256 = sha256_hex(content);
    sqlx::query(
        "INSERT INTO responses.stream_blocks
             (request_id, session_id, block_type_id, seq_ord, content,
              signature, wire_tool_use_id, content_sha256, byte_size)
         SELECT $1,
                (SELECT id FROM sessions.sessions WHERE session_uuid = $2::text::uuid),
                (SELECT id FROM responses.stream_block_types WHERE code = $3),
                $4, $5, NULL, NULL, $6, $7
         ON CONFLICT (request_id, seq_ord) DO NOTHING",
    )
    .bind(request_id)
    .bind(session_id)
    .bind(block_type)
    .bind(seq_ord)
    .bind(content)
    .bind(content_sha256)
    .bind(byte_size)
    .execute(pool)
    .await?;
    Ok(())
}

fn sse_streaming_body(chunks: &[&str]) -> String {
    let mut body = String::new();
    body.push_str("event: message_start\ndata: ");
    body.push_str(
        &json!({
            "type": "message_start",
            "message": {
                "id": "msg_blocks_test",
                "type": "message",
                "role": "assistant",
                "content": [],
                "model": "claude-sonnet-4-20250514",
                "stop_reason": null,
                "usage": {"input_tokens": 10, "output_tokens": 0}
            }
        })
        .to_string(),
    );
    body.push_str("\n\n");
    body.push_str("event: content_block_start\ndata: ");
    body.push_str(
        &json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}})
            .to_string(),
    );
    body.push_str("\n\n");
    for (i, chunk) in chunks.iter().enumerate() {
        body.push_str("event: content_block_delta\ndata: ");
        body.push_str(
            &json!({
                "type": "content_block_delta",
                "index": i,
                "delta": {"type": "text_delta", "text": chunk}
            })
            .to_string(),
        );
        body.push_str("\n\n");
    }
    body.push_str("event: content_block_stop\ndata: ");
    body.push_str(&json!({"type":"content_block_stop","index":0}).to_string());
    body.push_str("\n\n");
    body.push_str("event: message_delta\ndata: ");
    body.push_str(
        &json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens": 5}})
            .to_string(),
    );
    body.push_str("\n\n");
    body.push_str("event: message_stop\ndata: ");
    body.push_str(&json!({"type":"message_stop"}).to_string());
    body.push_str("\n\n");
    body
}

async fn wait_for_complete(
    client: &reqwest::Client,
    inspect_url: &str,
    id: u64,
    timeout: Duration,
) -> anyhow::Result<serde_json::Value> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if std::time::Instant::now() > deadline {
            anyhow::bail!("timed out waiting for request {id} to reach Complete");
        }
        let detail: serde_json::Value = client
            .get(format!("{inspect_url}/api/requests/{id}"))
            .send()
            .await?
            .json()
            .await?;
        if detail["status"] == "Complete" {
            return Ok(detail);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn fire_streaming_request_and_get_id(
    client: &reqwest::Client,
    proxy_url: &str,
    inspect_url: &str,
) -> anyhow::Result<u64> {
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header(
            "x-claude-code-session-id",
            "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11",
        )
        .json(
            &RequestBody::default()
                .stream(true)
                .content("stream me")
                .build(),
        )
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let _ = resp.bytes().await?;

    let list: serde_json::Value = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?
        .json()
        .await?;
    list.as_array()
        .and_then(|a| a.first())
        .and_then(|r| r["id"].as_u64())
        .ok_or_else(|| anyhow::anyhow!("no request recorded"))
}

#[path = "stream_chunks_db/row_roundtrip.rs"]
mod row_roundtrip;
#[path = "stream_chunks_db/split_phrase_assembly.rs"]
mod split_phrase_assembly;
#[path = "stream_chunks_db/stream_chunks_endpoint.rs"]
mod stream_chunks_endpoint;
