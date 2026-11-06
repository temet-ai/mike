use crate::anthropic_fixtures;

use anthropic_fixtures::{MockResponse, RequestBody, text_block};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
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
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(
                    MockResponse::new("msg_volatile_test")
                        .text("ack")
                        .usage(1, 1)
                        .build(),
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;
    mock_server
}

pub(super) async fn fire_and_fetch_prompt_sha256(
    client: &reqwest::Client,
    proxy_url: &str,
    inspect_url: &str,
    session_id: &str,
    block2_text: &str,
) -> anyhow::Result<Option<String>> {
    let body = RequestBody::default()
        .system(json!([
            text_block("billing-header"),
            text_block("identity preamble"),
            text_block(block2_text),
        ]))
        .content("ping")
        .max_tokens(16)
        .build();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", session_id)
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await?;
    anyhow::ensure!(resp.status().is_success(), "proxy POST failed: {resp:?}");

    let summaries: Vec<Value> = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?
        .json()
        .await?;
    let request_id = summaries
        .iter()
        .find(|s| s.get("session_id").and_then(|v| v.as_str()) == Some(session_id))
        .and_then(|s| s.get("id"))
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("request not surfaced for session {session_id}"))?;
    let detail: Value = client
        .get(format!("{inspect_url}/api/requests/{request_id}"))
        .send()
        .await?
        .json()
        .await?;
    Ok(detail["system_blocks"]["prompt_sha256"]
        .as_str()
        .map(|s| s.to_string()))
}
