use crate::anthropic_fixtures;
use crate::common;
use crate::db_helpers;

use anthropic_fixtures::{
    RequestBody, assistant_message, tool_result_block, tool_use_block, user_message,
};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn arm_compression_dedup(inspect_url: &str) -> anyhow::Result<()> {
    let resp = reqwest::Client::new()
        .patch(format!(
            "{inspect_url}/api/settings/compression_dedup.enabled"
        ))
        .json(&json!({"value": true}))
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "failed to arm compression_dedup.enabled: {}",
        resp.status()
    );
    Ok(())
}

fn plain_response_json(text: &str) -> Value {
    anthropic_fixtures::MockResponse::new("msg_dedup_test")
        .text(text)
        .usage(5, 1)
        .build()
}

async fn post_body(
    client: &reqwest::Client,
    proxy_url: &str,
    session_id: &str,
    body_bytes: &[u8],
) -> anyhow::Result<reqwest::Response> {
    Ok(client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test-key")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", session_id)
        .body(body_bytes.to_vec())
        .send()
        .await?)
}

async fn fetch_dedup_stats(
    client: &reqwest::Client,
    inspect_url: &str,
    session_id: &str,
) -> anyhow::Result<Value> {
    Ok(client
        .get(format!(
            "{inspect_url}/api/sessions/{session_id}/dedup-stats"
        ))
        .send()
        .await?
        .json()
        .await?)
}

#[tokio::test]
async fn dedup_records_hit_and_suppresses_the_duplicate_read() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(plain_response_json("ok"))
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let Some(proxy) = db_helpers::start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping dedup_records_hit_and_suppresses_the_duplicate_read"
        );
        return Ok(());
    };
    let proxy_url = proxy.proxy_url.clone();
    let inspect_url = proxy.inspect_url.clone();
    arm_compression_dedup(&inspect_url).await?;
    let client = reqwest::Client::new();
    let session_id = "33333333-3333-3333-3333-333333333333";

    let first_body = RequestBody::default()
        .max_tokens(100)
        .messages(json!([
            assistant_message(json!([tool_use_block(
                "t1",
                "Read",
                json!({"file_path": "/tmp/dedup_shadow_a.rs"})
            )])),
            user_message(json!([tool_result_block("t1", "fn main() {}")])),
        ]))
        .build();
    let first_bytes = serde_json::to_vec(&first_body)?;
    let first_resp = post_body(&client, &proxy_url, session_id, &first_bytes).await?;
    assert_eq!(first_resp.status(), 200);
    let _ = first_resp.bytes().await?;

    let second_body = RequestBody::default()
        .max_tokens(100)
        .messages(json!([
            assistant_message(json!([tool_use_block(
                "t1",
                "Read",
                json!({"file_path": "/tmp/dedup_shadow_a.rs"})
            )])),
            user_message(json!([tool_result_block("t1", "fn main() {}")])),
            assistant_message(json!([tool_use_block(
                "t2",
                "Read",
                json!({"file_path": "/tmp/dedup_shadow_a.rs"})
            )])),
            user_message(json!([tool_result_block("t2", "fn main() {}")])),
        ]))
        .build();
    let second_bytes = serde_json::to_vec(&second_body)?;
    let second_resp = post_body(&client, &proxy_url, session_id, &second_bytes).await?;
    assert_eq!(second_resp.status(), 200);
    let _ = second_resp.bytes().await?;

    let received = mock_server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("wiremock recorded no requests"))?;
    assert_eq!(received.len(), 2, "exactly two upstream requests expected");
    assert_eq!(
        received[0].body.as_slice(),
        first_bytes.as_slice(),
        "the first Read has nothing to dedup against and must forward byte-identical"
    );

    assert_suppression_stub_and_original_survive(&received[1].body)?;

    let stats = fetch_dedup_stats(&client, &inspect_url, session_id).await?;
    assert_eq!(
        stats["hit_count"], 1,
        "second identical Read must register as a would-dedup hit: {stats}"
    );
    assert!(
        stats["would_save_bytes"].as_u64().unwrap_or(0) > 0,
        "would_save_bytes must be positive on a hit: {stats}"
    );
    let per_path = stats["per_path"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("per_path must be an array"))?;
    assert!(
        per_path
            .iter()
            .any(|p| p["file_path"] == "/tmp/dedup_shadow_a.rs" && p["hit_count"] == 1),
        "per_path must attribute the hit to the duplicated file: {stats}"
    );

    proxy.test_db.cleanup().await?;
    Ok(())
}

fn assert_suppression_stub_and_original_survive(forwarded_body: &[u8]) -> anyhow::Result<()> {
    let forwarded: Value = serde_json::from_slice(forwarded_body)?;
    let messages = forwarded["messages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("forwarded body missing messages: {forwarded}"))?;
    let last_result = messages
        .last()
        .and_then(|m| m["content"].as_array())
        .and_then(|c| c.last())
        .and_then(|b| b["content"].as_str())
        .ok_or_else(|| anyhow::anyhow!("forwarded body missing last tool_result: {forwarded}"))?;
    assert!(
        last_result.contains("duplicate read suppressed"),
        "the repeated Read's result must carry the suppression stub, got: {last_result}"
    );
    assert!(
        last_result.contains("tool_use_id t1"),
        "the stub must cite the earlier tool_use_id so the model can find the content, got: {last_result}"
    );

    let first_result = messages
        .get(1)
        .and_then(|m| m["content"].as_array())
        .and_then(|c| c.first())
        .and_then(|b| b["content"].as_str())
        .ok_or_else(|| anyhow::anyhow!("forwarded body missing first tool_result: {forwarded}"))?;
    assert_eq!(
        first_result, "fn main() {}",
        "the original Read's content must survive — the stub points back to it"
    );
    Ok(())
}

fn build_read_request(target_path: &str) -> Value {
    RequestBody::default()
        .max_tokens(100)
        .messages(json!([
            assistant_message(json!([tool_use_block(
                "t1",
                "Read",
                json!({"file_path": target_path})
            )])),
            user_message(json!([tool_result_block("t1", "fn main() {}")])),
        ]))
        .build()
}

fn build_read_then_edit_request(target_path: &str) -> Value {
    RequestBody::default()
        .max_tokens(100)
        .messages(json!([
            assistant_message(json!([tool_use_block(
                "t1",
                "Read",
                json!({"file_path": target_path})
            )])),
            user_message(json!([tool_result_block("t1", "fn main() {}")])),
            assistant_message(json!([tool_use_block(
                "t2",
                "Edit",
                json!({
                    "file_path": target_path,
                    "old_string": "fn main() {}",
                    "new_string": "fn main() { changed(); }"
                })
            )])),
            user_message(json!([tool_result_block(
                "t2",
                "The file has been updated."
            )])),
        ]))
        .build()
}

fn build_read_edit_reread_request(target_path: &str) -> Value {
    RequestBody::default()
        .max_tokens(100)
        .messages(json!([
            assistant_message(json!([tool_use_block(
                "t1",
                "Read",
                json!({"file_path": target_path})
            )])),
            user_message(json!([tool_result_block("t1", "fn main() {}")])),
            assistant_message(json!([tool_use_block(
                "t2",
                "Edit",
                json!({
                    "file_path": target_path,
                    "old_string": "fn main() {}",
                    "new_string": "fn main() { changed(); }"
                })
            )])),
            user_message(json!([tool_result_block(
                "t2",
                "The file has been updated."
            )])),
            assistant_message(json!([tool_use_block(
                "t3",
                "Read",
                json!({"file_path": target_path})
            )])),
            user_message(json!([tool_result_block("t3", "fn main() {}")])),
        ]))
        .build()
}

#[tokio::test]
async fn dedup_shadow_edit_invalidates_so_next_read_is_not_a_hit() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(plain_response_json("ok"))
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let (proxy_url, inspect_url) = common::start_proxy(&mock_server.uri()).await?;
    let client = reqwest::Client::new();
    let session_id = "44444444-4444-4444-4444-444444444444";
    let target_path = "/tmp/dedup_shadow_b.rs";

    let resp = post_body(
        &client,
        &proxy_url,
        session_id,
        &serde_json::to_vec(&build_read_request(target_path))?,
    )
    .await?;
    assert_eq!(resp.status(), 200);
    let _ = resp.bytes().await?;

    let resp = post_body(
        &client,
        &proxy_url,
        session_id,
        &serde_json::to_vec(&build_read_then_edit_request(target_path))?,
    )
    .await?;
    assert_eq!(resp.status(), 200);
    let _ = resp.bytes().await?;

    let resp = post_body(
        &client,
        &proxy_url,
        session_id,
        &serde_json::to_vec(&build_read_edit_reread_request(target_path))?,
    )
    .await?;
    assert_eq!(resp.status(), 200);
    let _ = resp.bytes().await?;

    let stats = fetch_dedup_stats(&client, &inspect_url, session_id).await?;
    assert_eq!(
        stats["hit_count"], 0,
        "post-Edit re-read of the same content must NOT be counted as a hit: {stats}"
    );

    Ok(())
}
