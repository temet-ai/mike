use mike::store::persistence::PersistHandle;
use mike::store::persistence::proxy_repo::ProxyPersist as _;
use mike::store::persistence::remote_control::PATH_HEARTBEAT;

use crate::store::remote_control::support::{call_at, seed_channel, seed_session, shared_handle};

/// A registered session whose channel heartbeats; returns (cse_id, session row id).
async fn registered_channel(
    handle: &PersistHandle,
    lease_minutes: f64,
) -> anyhow::Result<(String, i64)> {
    let session = seed_session(handle, &format!("/repo/{}", uuid::Uuid::new_v4())).await?;
    let cse_id = seed_channel(handle, session).await?;
    let session_id: i64 = sqlx::query_scalar(
        "UPDATE remote_control.channels c SET session_id = s.id
           FROM sessions.sessions s
          WHERE c.cse_id = $1 AND s.session_uuid = $2
         RETURNING s.id",
    )
    .bind(&cse_id)
    .bind(session)
    .fetch_one(handle.pool())
    .await?;
    sqlx::query(
        "INSERT INTO workspace.session_registrations
             (session_id, project_id, cwd, registered_via, lease_expires_at)
         SELECT id, project_id, '/repo', 'passive', now() + make_interval(secs => $2 * 60)
           FROM sessions.sessions WHERE id = $1",
    )
    .bind(session_id)
    .bind(lease_minutes)
    .execute(handle.pool())
    .await?;
    Ok((cse_id, session_id))
}

async fn minutes_left(handle: &PersistHandle, session_id: i64) -> anyhow::Result<f64> {
    Ok(sqlx::query_scalar(
        "SELECT extract(epoch FROM lease_expires_at - now())::float8 / 60
           FROM workspace.session_registrations WHERE session_id = $1",
    )
    .bind(session_id)
    .fetch_one(handle.pool())
    .await?)
}

async fn heartbeat(handle: &PersistHandle, cse_id: &str) -> anyhow::Result<()> {
    handle
        .proxy()
        .record_remote_control_call(call_at(cse_id, "POST", PATH_HEARTBEAT))
        .await?;
    Ok(())
}

#[tokio::test]
async fn a_heartbeat_renews_a_lease_close_to_lapsing() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let (cse_id, session_id) = registered_channel(&handle, 3.0).await?;

    heartbeat(&handle, &cse_id).await?;

    assert!(minutes_left(&handle, session_id).await? > 14.0);
    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn a_heartbeat_leaves_a_lease_another_renewal_keeps_fresh() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let (cse_id, session_id) = registered_channel(&handle, 14.0).await?;
    heartbeat(&handle, &cse_id).await?;
    sqlx::query(
        "UPDATE workspace.session_registrations
            SET lease_expires_at = now() + interval '3 minutes' WHERE session_id = $1",
    )
    .bind(session_id)
    .execute(handle.pool())
    .await?;

    heartbeat(&handle, &cse_id).await?;
    assert!(
        minutes_left(&handle, session_id).await? < 4.0,
        "the lease seen fresh moments ago is not asked about again"
    );

    handle.note_lease(session_id, chrono::Utc::now());
    heartbeat(&handle, &cse_id).await?;
    assert!(
        minutes_left(&handle, session_id).await? > 14.0,
        "once the known lease nears its end the heartbeat renews it"
    );
    handle.pool().close().await;
    Ok(())
}
