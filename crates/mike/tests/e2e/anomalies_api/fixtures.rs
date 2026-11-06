use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub(super) fn prompt_sha256_for(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hex::encode(hasher.finalize())
}

pub(super) async fn mock_upstream_canned() -> MockServer {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "msg_anom_test",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "ack"}],
            "model": "claude-sonnet-4-20250514",
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .mount(&mock_server)
        .await;
    mock_server
}

pub(super) async fn insert_request_row(
    pool: &PgPool,
    session_id: &str,
    request_id: i64,
    system_blocks: Option<&Value>,
    prompt_sha256: Option<&str>,
    unknown_blocks: Option<&Value>,
) -> anyhow::Result<()> {
    let project_id: i64 = sqlx::query_scalar(
        "WITH ins AS (
             INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
             SELECT $1, $1, NOW(), NOW()
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
    .bind("anomalies-test-project")
    .fetch_one(pool)
    .await?;

    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, NOW(), NOW())
         ON CONFLICT (session_uuid) DO NOTHING",
    )
    .bind(session_id)
    .bind(project_id)
    .execute(pool)
    .await?;

    let (request_db_id, _request_created_at): (i64, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as(
            "INSERT INTO requests.requests (
            id, session_id, project_id,
            method_id, path, is_streaming, message_count, is_tool_loop,
            status_id,
            params,
            last_user_message, human_prompt,
            turn_type_id, turn_origin_id,
            created_at
         )
         OVERRIDING SYSTEM VALUE
         VALUES (
            $1, (SELECT id FROM sessions.sessions WHERE session_uuid = $2::text::uuid), $3,
            (SELECT id FROM requests.methods WHERE code = 'POST'),
            '/v1/messages', false, 1, false,
            (SELECT id FROM requests.statuses WHERE code = 'complete'),
            '{}'::jsonb,
            '', '',
            (SELECT id FROM requests.turn_types WHERE code = 'user_prompt'),
            (SELECT id FROM requests.turn_origins WHERE code = 'primary'),
            clock_timestamp()
         )
         RETURNING id, created_at",
        )
        .bind(request_id)
        .bind(session_id)
        .bind(project_id)
        .fetch_one(pool)
        .await?;

    if let (Some(sha), Some(blocks)) = (prompt_sha256, system_blocks) {
        sqlx::query(
            "INSERT INTO system_prompts.system_prompts (prompt_sha256, blocks, first_seen_at, last_seen_at)
             SELECT $1, $2, NOW(), NOW()
             WHERE NOT EXISTS (
                 SELECT 1 FROM system_prompts.system_prompts WHERE prompt_sha256 = $1
             )",
        )
        .bind(sha)
        .bind(blocks)
        .execute(pool)
        .await?;
    }

    sqlx::query(
        "INSERT INTO requests.system_prompts
            (request_id, prompt_sha256, system_prompt_id, override_id, override_applied,
             unknown_blocks)
         VALUES ($1, $2,
                 (SELECT id FROM system_prompts.system_prompts WHERE prompt_sha256 = $2),
                 NULL, false, $3)",
    )
    .bind(request_db_id)
    .bind(prompt_sha256)
    .bind(unknown_blocks)
    .execute(pool)
    .await?;

    Ok(())
}

pub(super) async fn fetch_anomalies(
    client: &reqwest::Client,
    inspect_url: &str,
) -> anyhow::Result<Vec<Value>> {
    let resp = client
        .get(format!("{inspect_url}/api/prompts/anomalies"))
        .send()
        .await?;
    let status = resp.status();
    let body: Value = resp.json().await?;
    anyhow::ensure!(
        status == 200,
        "GET /api/prompts/anomalies expected 200, got {status}: {body}"
    );
    let arr = body
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("anomalies response must be array, got {body}"))?
        .clone();
    Ok(arr)
}
