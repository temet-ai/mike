use super::harness::{
    ProxyHandle, insert_request, is_resync_required, is_row_changed, next_matching,
    seed_project_and_session, start_proxy_with_listener, wait_for_listener_pid,
    wait_for_new_listener_pid, ws_url,
};

use anyhow::Context as _;
use tokio_tungstenite::connect_async;

#[tokio::test]
async fn db_listener_reconnects_after_underlying_pg_session_is_killed() -> anyhow::Result<()> {
    let Some(handle) = start_proxy_with_listener().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping db_listener_smoke");
        return Ok(());
    };

    let (mut ws, _resp) = connect_async(ws_url(&handle.inspect_url)).await?;

    let (project_id, session_id) = seed_project_and_session(&handle.pool).await?;
    let req_id_pre_kill = insert_request(&handle.pool, project_id, &session_id).await?;

    let frame = next_matching(
        &mut ws,
        is_row_changed("requests", &req_id_pre_kill.to_string()),
    )
    .await
    .context("pre-kill requests INSERT frame")?;
    assert_eq!(
        frame["id"],
        req_id_pre_kill.to_string(),
        "pre-kill RowChanged frame must reference the seeded request id"
    );

    let listener_pid_before = wait_for_listener_pid(&handle.pool).await?;

    kill_listener_backend(&handle, listener_pid_before).await?;

    let listener_pid_after = wait_for_new_listener_pid(&handle.pool, listener_pid_before).await?;
    assert_ne!(
        listener_pid_before, listener_pid_after,
        "listener should reconnect on a fresh pg backend after termination"
    );

    next_matching(&mut ws, is_resync_required("recovery"))
        .await
        .context("ResyncRequired(recovery) frame after listener reconnect")?;

    let req_id_post_kill = insert_request(&handle.pool, project_id, &session_id).await?;
    let frame = next_matching(
        &mut ws,
        is_row_changed("requests", &req_id_post_kill.to_string()),
    )
    .await
    .context("post-reconnect requests INSERT frame")?;
    assert_eq!(
        frame["id"],
        req_id_post_kill.to_string(),
        "post-reconnect RowChanged frame must reference the second insert"
    );
    assert_eq!(frame["op"], "INSERT");
    assert_eq!(frame["session_id"], session_id);

    handle.cleanup().await?;
    Ok(())
}

async fn kill_listener_backend(handle: &ProxyHandle, listener_pid: i32) -> anyhow::Result<()> {
    use sqlx::Connection as _;
    let mut admin_conn = sqlx::PgConnection::connect(handle.test_db_url()).await?;
    let killed: bool = sqlx::query_scalar(
        "SELECT bool_or(
            CASE
                WHEN pid <> pg_backend_pid() THEN pg_terminate_backend(pid)
                ELSE FALSE
            END
        )
        FROM pg_stat_activity
        WHERE datname = current_database()
          AND query LIKE 'LISTEN %row_changed%'
          AND pid = $1",
    )
    .bind(listener_pid)
    .fetch_one(&mut admin_conn)
    .await?;
    assert!(
        killed,
        "expected to terminate listener pid {listener_pid}; pg_stat_activity didn't surface it"
    );
    drop(admin_conn);
    Ok(())
}
