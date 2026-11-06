use tokio_tungstenite::connect_async;

use super::fixtures::{
    fetch_flow_request_ids, insert_request, is_resync_required, is_row_changed_requests,
    kill_listener_backend, next_matching, seed_project_and_eligible_session,
    start_proxy_with_listener, wait_for_listener_pid, wait_for_new_listener_pid, ws_url,
};

#[tokio::test]
async fn session_flow_survives_listener_outage_with_ws_open() -> anyhow::Result<()> {
    let Some(handle) = start_proxy_with_listener().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping session_flow_listener_recovery");
        return Ok(());
    };

    let (mut ws, _resp) = connect_async(ws_url(&handle.inspect_url)).await?;
    let client = reqwest::Client::new();

    let (project_id, session_pk, session_id) =
        seed_project_and_eligible_session(&handle.pool).await?;

    let pre_kill_id = insert_request(&handle.pool, project_id, session_pk).await?;
    next_matching(&mut ws, is_row_changed_requests(&pre_kill_id.to_string()))
        .await
        .map_err(|e| anyhow::anyhow!("pre-kill requests INSERT frame: {e}"))?;
    let ids = fetch_flow_request_ids(&client, &handle.inspect_url, &session_id).await?;
    assert_eq!(
        ids,
        vec![pre_kill_id.to_string()],
        "pre-kill REST fetch must already show the sanity-check request"
    );

    let listener_pid_before = wait_for_listener_pid(&handle.pool).await?;
    kill_listener_backend(handle.test_db.database_url.as_str(), listener_pid_before).await?;

    let gap_id = insert_request(&handle.pool, project_id, session_pk).await?;

    let listener_pid_after = wait_for_new_listener_pid(&handle.pool, listener_pid_before).await?;
    assert_ne!(
        listener_pid_before, listener_pid_after,
        "listener should reconnect on a fresh pg backend after termination"
    );

    next_matching(&mut ws, is_resync_required("recovery"))
        .await
        .map_err(|e| anyhow::anyhow!("ResyncRequired(recovery) frame after reconnect: {e}"))?;

    let ids_after_recovery =
        fetch_flow_request_ids(&client, &handle.inspect_url, &session_id).await?;
    let mut expected = vec![pre_kill_id.to_string(), gap_id.to_string()];
    expected.sort();
    assert_eq!(
        ids_after_recovery, expected,
        "post-recovery REST fetch must include the gap-window insert whose NOTIFY was dropped (no lost update)"
    );

    let ids_resnapshot = fetch_flow_request_ids(&client, &handle.inspect_url, &session_id).await?;
    assert_eq!(
        ids_resnapshot, ids_after_recovery,
        "an immediate re-fetch must match the prior post-recovery snapshot exactly"
    );

    let post_recovery_id = insert_request(&handle.pool, project_id, session_pk).await?;
    next_matching(
        &mut ws,
        is_row_changed_requests(&post_recovery_id.to_string()),
    )
    .await
    .map_err(|e| anyhow::anyhow!("post-recovery requests INSERT frame: {e}"))?;
    let ids_final = fetch_flow_request_ids(&client, &handle.inspect_url, &session_id).await?;
    let mut expected_final = vec![
        pre_kill_id.to_string(),
        gap_id.to_string(),
        post_recovery_id.to_string(),
    ];
    expected_final.sort();
    assert_eq!(
        ids_final, expected_final,
        "final snapshot must carry all three requests: pre-kill, gap-window, and post-recovery"
    );

    handle.cleanup().await?;
    Ok(())
}
