use crate::db_helpers;

use super::support::{
    count_request_rows, fetch_session_row, mock_upstream_canned, send_request,
    wait_for_persisted_rows,
};

#[tokio::test]
async fn t3_single_request_increments_counter_exactly_once() -> anyhow::Result<()> {
    let mock_server = mock_upstream_canned().await;

    let Some(handle) = db_helpers::start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping T3");
        return Ok(());
    };
    let session_id = uuid::Uuid::new_v4().to_string();
    let client = reqwest::Client::new();

    send_request(&client, &handle.proxy_url, &session_id, "only").await?;
    wait_for_persisted_rows(&handle.pool, &session_id, 1).await?;

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let row = fetch_session_row(&handle.pool, &session_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("session row missing after T3 send"))?;
    let actual_rows = count_request_rows(&handle.pool, &session_id).await?;
    assert_eq!(
        row.request_count, 1,
        "T3: a single completed request must produce exactly request_count = 1"
    );
    assert_eq!(
        actual_rows, 1,
        "T3: a single completed request must produce exactly 1 row"
    );

    handle.shutdown.cancel();
    handle.test_db.cleanup().await?;
    Ok(())
}
