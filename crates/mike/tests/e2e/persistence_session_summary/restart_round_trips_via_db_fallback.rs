use crate::db_helpers;

use super::support::{
    fetch_session_row, mock_upstream_canned, send_request, wait_for_persisted_rows,
};
use serde_json::Value;

#[tokio::test]
async fn t5_session_summary_survives_restart_via_db_fallback() -> anyhow::Result<()> {
    let mock_server = mock_upstream_canned().await;

    let Some(handle) = db_helpers::start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping T5");
        return Ok(());
    };
    let (proxy_url_1, _inspect_url_1, shutdown_1, pool, test_db) = handle.into_parts();

    let session_id = uuid::Uuid::new_v4().to_string();
    let client = reqwest::Client::new();
    send_request(&client, &proxy_url_1, &session_id, "first").await?;
    wait_for_persisted_rows(&pool, &session_id, 1).await?;
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    send_request(&client, &proxy_url_1, &session_id, "second").await?;
    wait_for_persisted_rows(&pool, &session_id, 2).await?;

    let row_before = fetch_session_row(&pool, &session_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("session row missing pre-restart"))?;
    assert_eq!(row_before.request_count, 2);

    shutdown_1.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let database_url = test_db.database_url.clone();
    let (_proxy_url_2, inspect_url_2, shutdown_2) =
        db_helpers::start_proxy_with_db_at(&mock_server.uri(), &database_url).await?;

    let detail: Value = client
        .get(format!("{inspect_url_2}/api/sessions/{session_id}"))
        .send()
        .await?
        .json()
        .await?;
    let summary = detail
        .get("summary")
        .ok_or_else(|| anyhow::anyhow!("T5: session detail missing `summary`"))?;
    assert_eq!(
        summary.get("request_count").and_then(|v| v.as_u64()),
        Some(2),
        "T5: request_count must round-trip through restart via DB fallback"
    );
    assert_eq!(
        summary.get("last_status").and_then(|v| v.as_str()),
        Some("Complete"),
        "T5: last_status must round-trip via DB fallback"
    );
    let models = summary
        .get("models")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("T5: models missing or non-array"))?;
    let model_strs: Vec<&str> = models.iter().filter_map(|v| v.as_str()).collect();
    assert_eq!(
        model_strs,
        vec!["claude-sonnet-4-20250514"],
        "T5: models list must round-trip via DB fallback"
    );
    assert!(
        summary.get("project").map(|v| v.is_null()).unwrap_or(false),
        "T5: sentinel project must surface as null on the wire, got {:?}",
        summary.get("project")
    );
    let avg = summary
        .get("avg_latency_ms")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "T5: avg_latency_ms must be a number on the single-session \
                 detail fetch when completed requests exist, got {:?}",
                summary.get("avg_latency_ms")
            )
        })?;
    assert!(
        avg.is_finite() && avg >= 0.0,
        "T5: avg_latency_ms must be finite and non-negative, got {avg}"
    );

    shutdown_2.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
