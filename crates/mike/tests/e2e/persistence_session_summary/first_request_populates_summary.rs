use crate::db_helpers;

use super::support::{
    fetch_session_row, mock_upstream_canned, send_request, wait_for_persisted_rows,
};

#[tokio::test]
async fn t1_snapshot_populates_session_summary_on_first_request() -> anyhow::Result<()> {
    let mock_server = mock_upstream_canned().await;

    let Some(handle) = db_helpers::start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping T1");
        return Ok(());
    };
    let session_id = uuid::Uuid::new_v4().to_string();
    let client = reqwest::Client::new();

    send_request(&client, &handle.proxy_url, &session_id, "hello").await?;
    wait_for_persisted_rows(&handle.pool, &session_id, 1).await?;

    let row = fetch_session_row(&handle.pool, &session_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("session row missing after T1 send"))?;
    assert_eq!(row.request_count, 1, "T1: request_count must be 1");
    assert_eq!(
        row.last_status.as_deref(),
        Some("Complete"),
        "T1: last_status must reflect the terminal state"
    );
    assert_eq!(
        row.last_models,
        vec!["claude-sonnet-4-20250514".to_string()],
        "T1: rolling models list must carry the upstream model"
    );
    assert_eq!(
        row.project_path, "(no project)",
        "T1: a request without a project marker resolves to sentinel"
    );
    assert_eq!(
        row.first_seen, row.last_seen,
        "T1: first==last after the very first snapshot"
    );

    handle.shutdown.cancel();
    handle.test_db.cleanup().await?;
    Ok(())
}
