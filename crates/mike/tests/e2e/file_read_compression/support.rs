use crate::anthropic_fixtures;

use anthropic_fixtures::{
    RequestBody, assistant_message, text_block, tool_result_block, tool_use_block, user_message,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub(super) fn big_read_content(total_lines: usize) -> String {
    (1..=total_lines)
        .map(|i| format!("line {i:04} of a synthetic Read result padded out for size"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) async fn send_read_result_request(
    client: &reqwest::Client,
    proxy_url: &str,
    session_id: &str,
    tool_use_id: &str,
    file_path: &str,
    content: &str,
) -> anyhow::Result<reqwest::Response> {
    let body = RequestBody::default()
        .max_tokens(16)
        .system(json!([text_block("file-read-compression-test")]))
        .messages(json!([
            assistant_message(json!([tool_use_block(
                tool_use_id,
                "Read",
                json!({"file_path": file_path})
            )])),
            user_message(json!([tool_result_block(tool_use_id, content)])),
        ]))
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

pub(super) async fn fetch_manifest_row(
    pool: &sqlx::PgPool,
    session_id: &str,
) -> anyhow::Result<(Option<Vec<u8>>, Option<Vec<u8>>, Option<Value>)> {
    let row: (Option<Vec<u8>>, Option<Vec<u8>>, Option<Value>) = sqlx::query_as(
        "SELECT r.request_body, r.filtered_request_body, r.transformation_manifest
         FROM requests.requests r
         JOIN sessions.sessions s ON s.id = r.session_id
         WHERE s.session_uuid = $1::uuid
         ORDER BY r.id DESC
         LIMIT 1",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub(super) async fn fetch_delta_patch_len(
    pool: &sqlx::PgPool,
    request_id: i64,
) -> anyhow::Result<Option<i32>> {
    let row: Option<(Option<i32>,)> = sqlx::query_as(
        "SELECT octet_length(sent_patch_bytes)::int
         FROM requests.deltas
         WHERE request_id = $1
           AND sent_patch_start IS NOT NULL
         ORDER BY id
         LIMIT 1",
    )
    .bind(request_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|(v,)| v))
}

pub(super) fn sha256_of(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub(super) fn assert_manifest_chain_closes(manifest: &Value) -> anyhow::Result<()> {
    let original = manifest["original_sha256"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("manifest missing original_sha256"))?;
    let sent = manifest["sent_sha256"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("manifest missing sent_sha256"))?;
    let stages = manifest["stages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("manifest missing stages array"))?;
    let mut expected_input = original.to_string();
    for stage in stages {
        let input = stage["input_sha256"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("stage missing input_sha256: {stage:?}"))?;
        anyhow::ensure!(
            input == expected_input,
            "stage input_sha256 {input} does not chain to the previous output {expected_input}: {stage:?}"
        );
        expected_input = stage["output_sha256"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("stage missing output_sha256: {stage:?}"))?
            .to_string();
    }
    anyhow::ensure!(
        expected_input == sent,
        "final chain output {expected_input} does not equal manifest sent_sha256 {sent}"
    );
    Ok(())
}
