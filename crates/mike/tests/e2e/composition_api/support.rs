use crate::anthropic_fixtures;

use anthropic_fixtures::{RequestBody, cached_text_block, text_block};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub(super) async fn fire_request_and_get_session_and_request_ids(
    client: &reqwest::Client,
    proxy_url: &str,
    pool: &sqlx::PgPool,
    session_uuid: &str,
    prompt: &str,
) -> anyhow::Result<(i64, i64)> {
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", session_uuid)
        .header("content-type", "application/json")
        .json(
            &RequestBody::default()
                .content(prompt)
                .max_tokens(16)
                .build(),
        )
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "proxy request failed: {}",
        resp.status()
    );
    let _ = resp.bytes().await?;

    for _ in 0..50 {
        if let Some(row) = sqlx::query_as::<_, (i64, i64)>(
            "SELECT r.id, r.session_id FROM requests.requests r
               JOIN sessions.sessions s ON s.id = r.session_id
              WHERE s.session_uuid = $1::uuid
                AND r.human_prompt  = $2
              ORDER BY r.id DESC
              LIMIT 1",
        )
        .bind(session_uuid)
        .bind(prompt)
        .fetch_optional(pool)
        .await?
        {
            return Ok((row.1, row.0));
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    anyhow::bail!("request for prompt {prompt:?} never appeared in requests.requests")
}

pub(super) fn prompt_sha256_for(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hex::encode(hasher.finalize())
}

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

pub(super) async fn create_via_api(
    client: &reqwest::Client,
    inspect_url: &str,
    prompt_sha256: &str,
    label: &str,
    block2_text: &str,
) -> anyhow::Result<Value> {
    let resp = client
        .post(format!("{inspect_url}/api/prompts/compositions"))
        .json(&json!({
            "prompt_sha256": prompt_sha256,
            "label": label,
            "blocks": [
                {"text": "billing-header"},
                {"text": "identity preamble"},
                {"text": block2_text},
            ]
        }))
        .send()
        .await?;
    let status = resp.status();
    let body: Value = resp.json().await?;
    anyhow::ensure!(
        status == 201,
        "POST /api/prompts/compositions expected 201, got {status}: {body}"
    );
    Ok(body)
}

pub(super) async fn create_subagent_via_api(
    client: &reqwest::Client,
    inspect_url: &str,
    prompt_sha256: &str,
    label: &str,
    block2_text: &str,
    agent_name: &str,
) -> anyhow::Result<Value> {
    let resp = client
        .post(format!("{inspect_url}/api/prompts/compositions"))
        .json(&json!({
            "prompt_sha256": prompt_sha256,
            "label": label,
            "agent_name": agent_name,
            "blocks": [
                {"text": "billing-header"},
                {"text": "identity preamble"},
                {"text": block2_text},
            ]
        }))
        .send()
        .await?;
    let status = resp.status();
    let body: Value = resp.json().await?;
    anyhow::ensure!(
        status == 201,
        "POST /api/prompts/compositions expected 201, got {status}: {body}"
    );
    Ok(body)
}

pub(super) async fn find_upstream_body_for_session(
    upstream: &wiremock::MockServer,
    session_id: &str,
) -> anyhow::Result<Value> {
    let received = upstream.received_requests().await;
    let received = received.ok_or_else(|| anyhow::anyhow!("no requests recorded"))?;
    let last = received
        .iter()
        .rev()
        .find(|req| {
            req.headers
                .get("x-claude-code-session-id")
                .and_then(|h| h.to_str().ok())
                == Some(session_id)
        })
        .ok_or_else(|| anyhow::anyhow!("no request with matching session id"))?;
    Ok(serde_json::from_slice(&last.body)?)
}

pub(super) async fn force_mode(
    pool: &sqlx::PgPool,
    prompt_sha256: &str,
    new_mode: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE overrides.overrides o
         SET mode_id = (SELECT id FROM overrides.modes WHERE code = $2)
         WHERE o.prompt_sha256 = $1",
    )
    .bind(prompt_sha256)
    .bind(new_mode)
    .execute(pool)
    .await?;
    Ok(())
}
