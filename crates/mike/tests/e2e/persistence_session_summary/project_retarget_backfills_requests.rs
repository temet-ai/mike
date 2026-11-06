use crate::db_helpers;

use super::support::{
    fetch_session_row, mock_upstream_canned, send_request, wait_for_persisted_rows,
};

#[tokio::test]
async fn t4_project_retarget_backfills_earlier_requests() -> anyhow::Result<()> {
    let mock_server = mock_upstream_canned().await;

    let Some(handle) = db_helpers::start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping T4");
        return Ok(());
    };
    let session_id = uuid::Uuid::new_v4().to_string();
    let client = reqwest::Client::new();

    send_request(&client, &handle.proxy_url, &session_id, "no-marker").await?;
    wait_for_persisted_rows(&handle.pool, &session_id, 1).await?;
    let row_after_first = fetch_session_row(&handle.pool, &session_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("session row missing after T4 first send"))?;
    assert_eq!(
        row_after_first.project_path, "(no project)",
        "T4 step 1: first request without marker must resolve to sentinel"
    );

    let assign = client
        .patch(format!(
            "{}/api/sessions/{session_id}/project",
            handle.inspect_url
        ))
        .json(&serde_json::json!({"project_path": "/tmp/phase1-t4-proj"}))
        .send()
        .await?;
    anyhow::ensure!(
        assign.status().is_success(),
        "assign session project expected 2xx, got {}: {}",
        assign.status(),
        assign.text().await.unwrap_or_default()
    );

    send_request(&client, &handle.proxy_url, &session_id, "after-assign").await?;
    wait_for_persisted_rows(&handle.pool, &session_id, 2).await?;

    let mut tries = 50;
    let row_after_second = loop {
        let candidate = fetch_session_row(&handle.pool, &session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("session row missing after T4 second send"))?;
        if candidate.project_path != "(no project)" || tries == 0 {
            break candidate;
        }
        tries -= 1;
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    };

    assert!(
        row_after_second.project_path.contains("phase1-t4-proj"),
        "T4: session must resolve to the real project after marker arrives, got {}",
        row_after_second.project_path
    );

    let (sentinel_rows,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*)::bigint
         FROM requests.requests r
         JOIN projects.projects p ON p.id = r.project_id
         JOIN sessions.sessions s ON s.id = r.session_id
         WHERE s.session_uuid = $1::uuid
           AND p.project_path = $2",
    )
    .bind(&session_id)
    .bind("(no project)")
    .fetch_one(&handle.pool)
    .await?;
    assert_eq!(
        sentinel_rows, 0,
        "T4: no request row may still point at the sentinel after the back-fill"
    );

    let (real_rows,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*)::bigint
         FROM requests.requests r
         JOIN projects.projects p ON p.id = r.project_id
         JOIN sessions.sessions s ON s.id = r.session_id
         WHERE s.session_uuid = $1::uuid
           AND p.project_path = $2",
    )
    .bind(&session_id)
    .bind(&row_after_second.project_path)
    .fetch_one(&handle.pool)
    .await?;
    assert_eq!(
        real_rows, 2,
        "T4: both request rows must now point at the resolved project"
    );

    handle.shutdown.cancel();
    handle.test_db.cleanup().await?;
    Ok(())
}
