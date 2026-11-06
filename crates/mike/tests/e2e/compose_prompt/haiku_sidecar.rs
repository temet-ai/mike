use crate::anthropic_fixtures;
use crate::db_helpers;
use crate::upstream;

use anthropic_fixtures::{RequestBody, cached_text_block, text_block};
use serde_json::{Value, json};

use super::support::{
    insert_composition, latest_request_id_for_session, prompt_sha256, send_canonical_request,
    wait_for_request_detail,
};

async fn send_haiku_title_sidecar_request(
    client: &reqwest::Client,
    proxy_url: &str,
    session_id: &str,
) -> anyhow::Result<reqwest::Response> {
    let body = RequestBody::default()
        .model("claude-haiku-4-5")
        .max_tokens(64)
        .system(json!([
            text_block("billing-header"),
            cached_text_block("identity preamble"),
            json!({
                "type": "text",
                "text": "Generate a concise, sentence-case title for the conversation. Respond with JSON like {\"title\": \"...\"}.",
                "cache_control": {"type": "ephemeral", "ttl": "1h"}
            }),
        ]))
        .content("Summarize this conversation")
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

fn assert_haiku_sidecar_never_classified_as_composable(d_haiku: &Value) {
    assert_eq!(
        d_haiku.get("turn_origin").and_then(Value::as_str),
        Some("title_sidecar"),
        "test setup: haiku request must be classified as title_sidecar; \
         the helper body shape no longer matches extract_turn_origin"
    );
    assert_eq!(
        d_haiku.get("composition_applied"),
        Some(&Value::Bool(false)),
        "haiku title sidecar must never have composition_applied=true",
    );
    let haiku_prompt_sha256 = d_haiku
        .get("system_blocks")
        .and_then(|sb| sb.get("prompt_sha256"));
    assert!(
        matches!(haiku_prompt_sha256, None | Some(Value::Null)),
        "title sidecar early-return must NOT stamp a prompt_sha256; got {haiku_prompt_sha256:?}",
    );
}

fn assert_opus_is_first_composable_request(
    d_opus: &Value,
    opus_block2: &str,
) -> anyhow::Result<()> {
    assert_eq!(
        d_opus.get("composition_applied"),
        Some(&Value::Bool(true)),
        "opus is the first COMPOSABLE request of the session; composition_applied must be true",
    );
    let opus_prompt_sha256 = d_opus
        .get("system_blocks")
        .and_then(|v| v.get("prompt_sha256"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("opus request must carry prompt_sha256"))?;
    assert_eq!(opus_prompt_sha256, prompt_sha256(opus_block2));
    Ok(())
}

fn assert_upstream_saw_haiku_passthrough_and_opus_templated(
    received: &[wiremock::Request],
    new_block2: &str,
) -> anyhow::Result<()> {
    assert_eq!(received.len(), 2, "both requests must reach the upstream");
    let mut saw_haiku_passthrough = false;
    let mut saw_opus_templated = false;
    for req in received {
        let body: Value = serde_json::from_slice(&req.body)?;
        let model = body.get("model").and_then(Value::as_str).unwrap_or("");
        if model.contains("haiku") {
            let block2_text = body["system"][2]["text"].as_str().unwrap_or("");
            assert!(
                block2_text.contains("Generate a concise, sentence-case title"),
                "haiku request must be byte-passthrough; block 2 was rewritten to {block2_text:?}",
            );
            assert!(
                !block2_text.contains(new_block2),
                "haiku must NEVER be rewritten with the opus composition",
            );
            saw_haiku_passthrough = true;
        } else {
            assert_eq!(
                body["system"][2]["text"].as_str(),
                Some(new_block2),
                "opus block 2 text must be the composition's new_block2",
            );
            assert_eq!(
                body["system"][2]["cache_control"]["type"].as_str(),
                Some("ephemeral"),
            );
            assert_eq!(
                body["system"][2]["cache_control"]["ttl"].as_str(),
                Some("1h"),
            );
            saw_opus_templated = true;
        }
    }
    assert!(
        saw_haiku_passthrough,
        "missing haiku passthrough at upstream"
    );
    assert!(
        saw_opus_templated,
        "missing opus templated request at upstream"
    );
    Ok(())
}

#[tokio::test]
async fn haiku_title_sidecar_does_not_consume_first_in_session_slot() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose").await;
    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let opus_block2 = "opus-main-agent-role-body";
    let new_block2 = "REWRITTEN-OPUS-BODY";
    insert_composition(
        &pool,
        &prompt_sha256(opus_block2),
        "opus-main-agent",
        new_block2,
        "template",
    )
    .await?;

    let (proxy_url, inspect_url, _shutdown) =
        db_helpers::start_proxy_with_db_at_composed(&upstream.uri(), &test_db.database_url).await?;
    let client = reqwest::Client::new();
    let session_id = "deadbeef-7777-4888-8999-aaaaaaaaaaaa";

    let r1 = send_haiku_title_sidecar_request(&client, &proxy_url, session_id).await?;
    assert_eq!(r1.status(), 200);
    let id_haiku = latest_request_id_for_session(&client, &inspect_url, session_id).await?;
    let d_haiku = wait_for_request_detail(&client, &inspect_url, id_haiku).await?;
    assert_haiku_sidecar_never_classified_as_composable(&d_haiku);

    let r2 = send_canonical_request(&client, &proxy_url, session_id, opus_block2).await?;
    assert_eq!(r2.status(), 200);
    let summaries: Vec<Value> = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?
        .json()
        .await?;
    let id_opus = summaries
        .iter()
        .filter(|s| s.get("session_id").and_then(Value::as_str) == Some(session_id))
        .filter_map(|s| s.get("id").and_then(Value::as_u64))
        .max()
        .ok_or_else(|| anyhow::anyhow!("no requests for session"))?;
    assert_ne!(id_opus, id_haiku, "second request must have a fresh id");
    let d_opus = wait_for_request_detail(&client, &inspect_url, id_opus).await?;
    assert_opus_is_first_composable_request(&d_opus, opus_block2)?;

    let received = upstream.received_requests().await;
    let received = received.ok_or_else(|| anyhow::anyhow!("no requests"))?;
    assert_upstream_saw_haiku_passthrough_and_opus_templated(&received, new_block2)?;

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn replace_mode_row_increments_replace_unsupported_counter() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose").await;
    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let block2 = "replace-mode-incoming-text";
    let rewritten_text = "WOULD-BE-REPLACED-IF-V1-SUPPORTED-IT";
    insert_composition(
        &pool,
        &prompt_sha256(block2),
        "legacy",
        rewritten_text,
        "replace",
    )
    .await?;

    let (proxy_url, inspect_url, shutdown) =
        db_helpers::start_proxy_with_db_at_composed(&upstream.uri(), &test_db.database_url).await?;
    let client = reqwest::Client::new();
    let session_id = "11111111-cafe-4444-8888-feedfeedfeed";

    let resp = send_canonical_request(&client, &proxy_url, session_id, block2).await?;
    assert_eq!(resp.status(), 200);

    let received = upstream.received_requests().await;
    let received = received.ok_or_else(|| anyhow::anyhow!("no requests"))?;
    assert_eq!(received.len(), 1);
    let upstream_body: Value = serde_json::from_slice(&received[0].body)?;
    assert_eq!(
        upstream_body["system"][2]["text"].as_str(),
        Some(block2),
        "replace-mode row MUST NOT rewrite bytes upstream in V1; \
         got {:?}",
        upstream_body["system"][2]["text"]
    );

    let request_id = latest_request_id_for_session(&client, &inspect_url, session_id).await?;
    let detail = wait_for_request_detail(&client, &inspect_url, request_id).await?;
    let projection = detail
        .get("system_blocks")
        .ok_or_else(|| anyhow::anyhow!("missing system_blocks"))?;
    let actual_prompt_sha256 = projection
        .get("prompt_sha256")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("prompt_sha256 absent"))?;
    assert_eq!(actual_prompt_sha256, prompt_sha256(block2));
    assert_eq!(detail.get("composition_applied"), Some(&Value::Bool(false)));

    let metrics_text = client
        .get(format!("{inspect_url}/api/metrics"))
        .send()
        .await?
        .text()
        .await?;
    let mut found = false;
    for line in metrics_text.lines() {
        if line.starts_with("claude_proxy_compositions_total")
            && line.contains("outcome=\"replace_unsupported\"")
        {
            let value: f64 = line
                .split_whitespace()
                .next_back()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| anyhow::anyhow!("could not parse metrics line: {line}"))?;
            assert!(
                value >= 1.0,
                "replace_unsupported counter must be at least 1, got {value} (line: {line})"
            );
            found = true;
            break;
        }
    }
    assert!(
        found,
        "metrics did not surface claude_proxy_compositions_total{{outcome=\"replace_unsupported\"}}; \
         text was:\n{metrics_text}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
