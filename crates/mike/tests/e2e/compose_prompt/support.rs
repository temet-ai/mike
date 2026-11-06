use crate::anthropic_fixtures;

use anthropic_fixtures::{RequestBody, cached_text_block, text_block};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub(super) async fn send_canonical_request(
    client: &reqwest::Client,
    proxy_url: &str,
    session_id: &str,
    block2_text: &str,
) -> anyhow::Result<reqwest::Response> {
    let body = RequestBody::default()
        .max_tokens(16)
        .system(json!([
            text_block("billing-header"),
            cached_text_block("identity preamble"),
            json!({"type": "text", "text": block2_text, "cache_control": {"type": "ephemeral", "ttl": "1h"}}),
        ]))
        .content("ping")
        .build();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", session_id)
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await?;
    Ok(resp)
}

pub(super) async fn send_string_form_request(
    client: &reqwest::Client,
    proxy_url: &str,
    session_id: &str,
) -> anyhow::Result<reqwest::Response> {
    let body = RequestBody::default()
        .max_tokens(16)
        .system("string-form-system-prompt")
        .content("hello")
        .build();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", session_id)
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await?;
    Ok(resp)
}

pub(super) async fn send_two_block_request(
    client: &reqwest::Client,
    proxy_url: &str,
    session_id: &str,
) -> anyhow::Result<reqwest::Response> {
    let body = RequestBody::default()
        .max_tokens(16)
        .system(json!([text_block("billing"), text_block("preamble")]))
        .content("hello")
        .build();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", session_id)
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await?;
    Ok(resp)
}

pub(super) async fn send_non_text_block_request(
    client: &reqwest::Client,
    proxy_url: &str,
    session_id: &str,
) -> anyhow::Result<reqwest::Response> {
    let body = RequestBody::default()
        .max_tokens(16)
        .system(json!([
            text_block("x-anthropic-billing-header: cch=abc;"),
            json!({"type": "image", "source": {"type": "base64", "data": ""}}),
            text_block("role body"),
        ]))
        .content("hello")
        .build();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", session_id)
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await?;
    Ok(resp)
}

pub(super) fn prompt_sha256(block2_text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(block2_text.as_bytes());
    hex::encode(hasher.finalize())
}

pub(super) async fn wait_for_request_detail(
    client: &reqwest::Client,
    inspect_url: &str,
    request_id: u64,
) -> anyhow::Result<Value> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let resp = client
            .get(format!("{inspect_url}/api/requests/{request_id}"))
            .send()
            .await?;
        if resp.status() == 200 {
            return Ok(resp.json::<Value>().await?);
        }
        if std::time::Instant::now() >= deadline {
            anyhow::bail!(
                "timed out waiting for /api/requests/{request_id}; last status {}",
                resp.status()
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

pub(super) async fn latest_request_id_for_session(
    client: &reqwest::Client,
    inspect_url: &str,
    session_id: &str,
) -> anyhow::Result<u64> {
    let summaries: Vec<Value> = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?
        .json()
        .await?;
    let id = summaries
        .iter()
        .find(|s| s.get("session_id").and_then(|v| v.as_str()) == Some(session_id))
        .and_then(|s| s.get("id"))
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("no request found for session_id={session_id}"))?;
    Ok(id)
}

pub(super) async fn insert_composition(
    pool: &sqlx::PgPool,
    prompt_sha256: &str,
    label: &str,
    block2_text: &str,
    mode: &str,
) -> anyhow::Result<()> {
    let blocks = json!([
        text_block("billing-header"),
        text_block("identity preamble"),
        text_block(block2_text),
    ]);
    sqlx::query(
        "INSERT INTO system_prompts.system_prompts
            (prompt_sha256, blocks, first_seen_at, last_seen_at)
         SELECT $1, $2, NOW(), NOW()
         WHERE NOT EXISTS (
             SELECT 1 FROM system_prompts.system_prompts WHERE prompt_sha256 = $1
         )",
    )
    .bind(prompt_sha256)
    .bind(&blocks)
    .execute(pool)
    .await?;

    let existing: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM overrides.overrides WHERE prompt_sha256 = $1 LIMIT 1")
            .bind(prompt_sha256)
            .fetch_optional(pool)
            .await?;
    anyhow::ensure!(
        existing.is_none(),
        "insert_composition: composition for prompt_sha256 {prompt_sha256:?} already exists — \
         duplicate seed in same test fixture indicates an isolation bug"
    );

    let inserted = sqlx::query(
        "INSERT INTO overrides.overrides
            (prompt_sha256, system_prompt_id, label, blocks, mode_id)
         SELECT $1,
                (SELECT id FROM system_prompts.system_prompts WHERE prompt_sha256 = $1),
                $2, $3,
                (SELECT id FROM overrides.modes WHERE code = $4)",
    )
    .bind(prompt_sha256)
    .bind(label)
    .bind(&blocks)
    .bind(mode)
    .execute(pool)
    .await?;
    anyhow::ensure!(
        inserted.rows_affected() == 1,
        "insert_composition: INSERT into overrides.overrides affected {} rows (expected 1) \
         for prompt_sha256 {prompt_sha256:?}; mode code {mode:?} may be unknown",
        inserted.rows_affected()
    );
    Ok(())
}
