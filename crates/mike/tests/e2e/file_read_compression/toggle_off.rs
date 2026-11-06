use crate::db_helpers;
use crate::upstream;

use serde_json::Value;

use super::support::{
    assert_manifest_chain_closes, big_read_content, fetch_manifest_row,
    latest_request_id_for_session, send_read_result_request, wait_for_request_detail,
};

#[tokio::test]
async fn file_read_compression_toggle_off_forwards_byte_identical() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compression_off").await;
    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    sqlx::query(
        "UPDATE config.feature_toggles SET enabled = false WHERE key = 'file_read_compression'",
    )
    .execute(&pool)
    .await?;

    let (proxy_url, inspect_url, shutdown) =
        db_helpers::start_proxy_with_db_at(&upstream.uri(), &test_db.database_url).await?;
    let client = reqwest::Client::new();
    let session_id = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeff2";
    let content = big_read_content(200);

    let resp = send_read_result_request(
        &client,
        &proxy_url,
        session_id,
        "toolu_01compressB",
        "/tmp/mike_compression_test_off.txt",
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
        "toggle off must not reduce: {forwarded_result_text}"
    );
    assert_eq!(forwarded_result_text, content);

    let (request_body, filtered_request_body, manifest) =
        fetch_manifest_row(&pool, session_id).await?;
    let original_bytes = request_body.ok_or_else(|| anyhow::anyhow!("request_body missing"))?;
    assert_eq!(
        original_bytes, forwarded_bytes,
        "toggle off: forwarded bytes must equal the client's original bytes"
    );
    assert!(
        filtered_request_body.is_none(),
        "toggle off must not populate filtered_request_body"
    );

    let manifest = manifest.ok_or_else(|| anyhow::anyhow!("transformation_manifest missing"))?;
    let stages = manifest["stages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("stages missing"))?;
    assert!(
        !stages.iter().any(|s| s["stage"] == "compression.file_read"),
        "toggle off must not record a compression.file_read stage: {stages:?}"
    );
    assert_eq!(manifest["original_sha256"], manifest["sent_sha256"]);
    assert_manifest_chain_closes(&manifest)?;

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
