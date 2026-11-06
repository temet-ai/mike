use crate::db_helpers;

use super::support::{
    count_request_rows, fetch_session_row, mock_upstream_canned, send_request,
    wait_for_persisted_rows,
};

#[tokio::test]
async fn t2_snapshot_increments_count_to_n_and_tracks_max_last_seen() -> anyhow::Result<()> {
    let mock_server = mock_upstream_canned().await;

    let Some(handle) = db_helpers::start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping T2");
        return Ok(());
    };
    let session_id = uuid::Uuid::new_v4().to_string();
    let client = reqwest::Client::new();

    const N: i32 = 5;
    let initial_first_seen = {
        send_request(&client, &handle.proxy_url, &session_id, "first").await?;
        wait_for_persisted_rows(&handle.pool, &session_id, 1).await?;
        let row = fetch_session_row(&handle.pool, &session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("session row missing after first T2 send"))?;
        row.first_seen
    };

    for i in 1..N {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        send_request(
            &client,
            &handle.proxy_url,
            &session_id,
            &format!("turn-{i}"),
        )
        .await?;
        wait_for_persisted_rows(&handle.pool, &session_id, (i + 1) as i64).await?;
    }

    let row = fetch_session_row(&handle.pool, &session_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("session row missing after N T2 sends"))?;
    assert_eq!(
        row.request_count, N,
        "T2: request_count must equal seeded N={N}"
    );
    let actual_rows = count_request_rows(&handle.pool, &session_id).await?;
    assert_eq!(
        actual_rows, N as i64,
        "T2: counter must match actual request rows"
    );
    assert_eq!(
        row.first_seen, initial_first_seen,
        "T2: first_seen must NOT advance with subsequent requests"
    );
    assert!(
        row.last_seen > initial_first_seen,
        "T2: last_seen must advance from first_seen after subsequent requests"
    );

    handle.shutdown.cancel();
    handle.test_db.cleanup().await?;
    Ok(())
}
