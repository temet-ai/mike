use crate::anthropic_fixtures;
use crate::db_helpers;
use crate::upstream;

use anthropic_fixtures::{
    RequestBody, assistant_message, text_block, tool_use_block, user_message,
};
use serde_json::{Value, json};

use super::support::{
    assert_manifest_chain_closes, big_read_content, fetch_delta_patch_len, fetch_manifest_row,
    latest_request_id_for_session, send_read_result_request, sha256_of, wait_for_request_detail,
};

#[tokio::test]
async fn file_read_compression_toggle_on_shadow_mode_forwards_byte_identical() -> anyhow::Result<()>
{
    let upstream = upstream::canned_json_upstream("msg_compression_on").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();
    let session_id = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeff1";
    let content = big_read_content(200);

    let resp = send_read_result_request(
        &client,
        &proxy_url,
        session_id,
        "toolu_01compressA",
        "/tmp/mike_compression_test_on.txt",
        &content,
    )
    .await?;
    assert_eq!(resp.status(), 200);

    let request_id = latest_request_id_for_session(&client, &inspect_url, session_id).await?;
    wait_for_request_detail(&client, &inspect_url, request_id).await?;

    let received = upstream.received_requests().await;
    let received = received.ok_or_else(|| anyhow::anyhow!("no requests"))?;
    assert_eq!(received.len(), 1);
    let forwarded_bytes = received[0].body.clone();
    let forwarded_json: Value = serde_json::from_slice(&forwarded_bytes)?;
    let forwarded_result_text = forwarded_json["messages"][1]["content"][0]["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("forwarded tool_result content missing or not a string"))?;
    assert!(
        !forwarded_result_text.contains("[[mike:file_read_reduced]]"),
        "shadow mode must never forward a reduction marker: {forwarded_result_text}"
    );
    assert_eq!(
        forwarded_result_text, content,
        "shadow mode must forward the tool_result content byte-identical"
    );

    let (request_body, filtered_request_body, manifest) =
        fetch_manifest_row(&pool, session_id).await?;
    let original_bytes = request_body.ok_or_else(|| anyhow::anyhow!("request_body missing"))?;
    assert_eq!(
        original_bytes, forwarded_bytes,
        "shadow mode: forwarded bytes must equal the client's original bytes"
    );
    assert!(
        filtered_request_body.is_none(),
        "shadow mode must not populate filtered_request_body"
    );

    let delta_patch_len = fetch_delta_patch_len(&pool, request_id as i64).await?;
    assert!(
        delta_patch_len.is_none(),
        "shadow mode must not record a sent-patch delta — got {delta_patch_len:?}"
    );

    let sent_payload: Value = client
        .get(format!(
            "{inspect_url}/api/requests/{request_id}/body/request_sent"
        ))
        .send()
        .await?
        .json()
        .await?;
    let sent_text = sent_payload["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("request_sent body missing text"))?;
    assert_eq!(
        sent_text.as_bytes(),
        forwarded_bytes.as_slice(),
        "request_sent must reconstruct the exact bytes forwarded upstream"
    );
    assert_eq!(
        sent_text.as_bytes(),
        original_bytes.as_slice(),
        "shadow mode: sent bytes equal the original bytes"
    );

    let manifest = manifest.ok_or_else(|| anyhow::anyhow!("transformation_manifest missing"))?;
    assert_eq!(
        manifest["original_sha256"].as_str(),
        Some(sha256_of(&original_bytes).as_str())
    );
    assert_eq!(
        manifest["sent_sha256"].as_str(),
        Some(sha256_of(&forwarded_bytes).as_str())
    );
    assert_eq!(manifest["original_sha256"], manifest["sent_sha256"]);
    assert_manifest_chain_closes(&manifest)?;

    let stages = manifest["stages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("stages missing"))?;
    assert!(
        !stages.iter().any(|s| s["stage"] == "compression.file_read"),
        "shadow mode must not record a compression.file_read stage: {stages:?}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn file_read_compression_repeated_identical_history_forwards_byte_identical_both_times()
-> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compression_determinism").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();
    let session_id = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeff3";
    let content = big_read_content(200);

    let r1 = send_read_result_request(
        &client,
        &proxy_url,
        session_id,
        "toolu_01compressC",
        "/tmp/mike_compression_test_determinism.txt",
        &content,
    )
    .await?;
    assert_eq!(r1.status(), 200);
    let r1_id = latest_request_id_for_session(&client, &inspect_url, session_id).await?;
    wait_for_request_detail(&client, &inspect_url, r1_id).await?;

    let r2 = send_read_result_request(
        &client,
        &proxy_url,
        session_id,
        "toolu_01compressC",
        "/tmp/mike_compression_test_determinism.txt",
        &content,
    )
    .await?;
    assert_eq!(r2.status(), 200);
    let r2_id = latest_request_id_for_session(&client, &inspect_url, session_id).await?;
    assert_ne!(r2_id, r1_id, "second request must mint a new id");
    wait_for_request_detail(&client, &inspect_url, r2_id).await?;

    let received = upstream.received_requests().await;
    let received = received.ok_or_else(|| anyhow::anyhow!("no requests"))?;
    assert_eq!(received.len(), 2);
    let forwarded_r1 = received[0].body.clone();
    let forwarded_r2 = received[1].body.clone();

    let r1_result_text: Value = serde_json::from_slice(&forwarded_r1)?;
    let r2_result_text: Value = serde_json::from_slice(&forwarded_r2)?;
    let r1_text = r1_result_text["messages"][1]["content"][0]["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("r1 forwarded content missing"))?;
    let r2_text = r2_result_text["messages"][1]["content"][0]["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("r2 forwarded content missing"))?;
    assert!(
        !r1_text.contains("[[mike:file_read_reduced]]"),
        "shadow mode must never forward a reduction marker on the first turn"
    );
    assert_eq!(r1_text, content, "first turn must forward byte-identical");
    assert!(
        !r2_text.contains("[[mike:file_read_reduced]]"),
        "file_read_compression's marker must never appear on the second turn either"
    );

    let (_req2_body, _filtered2, manifest2) = fetch_manifest_row(&pool, session_id).await?;
    let manifest2 = manifest2.ok_or_else(|| anyhow::anyhow!("transformation_manifest missing"))?;
    assert_manifest_chain_closes(&manifest2)?;
    let stages2 = manifest2["stages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("stages missing"))?;
    assert!(
        !stages2
            .iter()
            .any(|s| s["stage"] == "compression.file_read"),
        "second (continuation) request must not record a compression stage in shadow mode: {stages2:?}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn file_read_compression_shadow_mode_preserves_cache_control_bearing_message()
-> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compression_cache_control").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();
    let session_id = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeff4";
    let content = big_read_content(200);
    let tool_use_id = "toolu_01compressCache";

    let body = RequestBody::default()
        .max_tokens(16)
        .system(json!([text_block(
            "file-read-compression-cache-control-test"
        )]))
        .messages(json!([
            assistant_message(json!([tool_use_block(
                tool_use_id,
                "Read",
                json!({"file_path": "/tmp/mike_compression_test_cache.txt"})
            )])),
            user_message(json!([{
                "type": "tool_result",
                "tool_use_id": tool_use_id,
                "content": content,
                "cache_control": {"type": "ephemeral"}
            }])),
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
    assert_eq!(resp.status(), 200);

    let request_id = latest_request_id_for_session(&client, &inspect_url, session_id).await?;
    wait_for_request_detail(&client, &inspect_url, request_id).await?;

    let received = upstream.received_requests().await;
    let received = received.ok_or_else(|| anyhow::anyhow!("no requests"))?;
    assert_eq!(received.len(), 1);
    let forwarded_bytes = received[0].body.clone();
    let forwarded_json: Value = serde_json::from_slice(&forwarded_bytes)?;
    assert_eq!(
        forwarded_json["messages"][1]["content"][0]["cache_control"],
        json!({"type": "ephemeral"}),
        "sanity: upstream must still see cache_control on the tool_result block"
    );
    let forwarded_result_text = forwarded_json["messages"][1]["content"][0]["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("forwarded tool_result content missing or not a string"))?;
    assert!(
        !forwarded_result_text.contains("[[mike:file_read_reduced]]"),
        "shadow mode must never forward a reduction marker: {forwarded_result_text}"
    );
    assert_eq!(
        forwarded_result_text, content,
        "shadow mode must forward the tool_result content byte-identical"
    );

    let (request_body, filtered_request_body, _manifest) =
        fetch_manifest_row(&pool, session_id).await?;
    let original_bytes = request_body.ok_or_else(|| anyhow::anyhow!("request_body missing"))?;
    assert!(filtered_request_body.is_none());

    let sent_payload: Value = client
        .get(format!(
            "{inspect_url}/api/requests/{request_id}/body/request_sent"
        ))
        .send()
        .await?
        .json()
        .await?;
    let sent_text = sent_payload["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("request_sent body missing text"))?;
    assert_eq!(
        sent_text.as_bytes(),
        forwarded_bytes.as_slice(),
        "request_sent must reconstruct byte-exactly, including the cache_control-bearing block"
    );
    assert_eq!(
        sent_text.as_bytes(),
        original_bytes.as_slice(),
        "shadow mode: sent bytes equal the original bytes"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
