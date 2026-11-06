use chrono::{SubsecRound, Utc};

use mike::store::persistence::remote_control::{
    PATH_HEARTBEAT, RemoteControlCall, RemoteControlSignals,
};

use crate::store::remote_control::support::{call_at, shared_handle, unique};

#[tokio::test]
async fn first_call_creates_the_channel_and_later_calls_reuse_it() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_reuse");

    let first = handle
        .record_remote_control_call(call_at(&cse_id, "POST", PATH_HEARTBEAT))
        .await?;
    let second = handle
        .record_remote_control_call(call_at(&cse_id, "POST", "/worker/events"))
        .await?;
    assert_ne!(first, second, "each call gets its own row");

    let channels = sqlx::query!(
        r#"SELECT id, cse_id, first_seen_at, last_seen_at FROM remote_control.channels
             WHERE cse_id = $1"#,
        cse_id
    )
    .fetch_all(handle.pool())
    .await?;
    assert_eq!(channels.len(), 1, "both calls share one channel");
    assert_eq!(channels[0].cse_id, cse_id);
    assert!(
        channels[0].last_seen_at >= channels[0].first_seen_at,
        "last_seen_at advances while first_seen_at stays put"
    );

    let call_channels = sqlx::query_scalar!(
        r#"SELECT DISTINCT channel_id AS "channel_id!" FROM remote_control.calls
             WHERE channel_id = $1"#,
        channels[0].id
    )
    .fetch_all(handle.pool())
    .await?;
    assert_eq!(call_channels, vec![channels[0].id]);

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn heartbeat_signals_land_on_the_channel_row() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_hb");

    handle
        .record_remote_control_call(RemoteControlCall {
            signals: RemoteControlSignals {
                client_session_id: Some(&cse_id),
                worker_epoch: Some(5),
                idle_seconds: Some(143),
                current_interval_seconds: Some(20),
                worker_status: None,
                ..RemoteControlSignals::default()
            },
            ..call_at(&cse_id, "POST", PATH_HEARTBEAT)
        })
        .await?;

    let row = sqlx::query!(
        r#"SELECT client_session_id, worker_epoch, last_idle_seconds,
                  last_heartbeat_interval_seconds, last_heartbeat_at
             FROM remote_control.channels WHERE cse_id = $1"#,
        cse_id
    )
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(row.client_session_id.as_deref(), Some(cse_id.as_str()));
    assert_eq!(row.worker_epoch, Some(5));
    assert_eq!(row.last_idle_seconds, Some(143));
    assert_eq!(row.last_heartbeat_interval_seconds, Some(20));
    assert!(row.last_heartbeat_at.is_some());

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn an_idle_session_records_zero_rather_than_nothing() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_busy");

    handle
        .record_remote_control_call(RemoteControlCall {
            signals: RemoteControlSignals {
                idle_seconds: Some(0),
                ..RemoteControlSignals::default()
            },
            ..call_at(&cse_id, "POST", PATH_HEARTBEAT)
        })
        .await?;

    let idle = sqlx::query_scalar!(
        r#"SELECT last_idle_seconds FROM remote_control.channels WHERE cse_id = $1"#,
        cse_id
    )
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(idle, Some(0));

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn a_later_call_never_rewinds_an_earlier_rollup_field() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_order");

    let at = Utc::now().trunc_subsecs(6);
    handle
        .record_remote_control_call(RemoteControlCall {
            created_at: at,
            signals: RemoteControlSignals {
                worker_epoch: Some(5),
                idle_seconds: Some(42),
                ..RemoteControlSignals::default()
            },
            ..call_at(&cse_id, "POST", PATH_HEARTBEAT)
        })
        .await?;
    handle
        .record_remote_control_call(RemoteControlCall {
            created_at: at + chrono::Duration::seconds(1),
            signals: RemoteControlSignals {
                worker_epoch: Some(5),
                worker_status: Some("requires_action"),
                ..RemoteControlSignals::default()
            },
            ..call_at(&cse_id, "PUT", "/worker")
        })
        .await?;
    handle
        .record_remote_control_call(RemoteControlCall {
            created_at: at - chrono::Duration::seconds(30),
            signals: RemoteControlSignals {
                worker_epoch: Some(5),
                idle_seconds: Some(1),
                ..RemoteControlSignals::default()
            },
            ..call_at(&cse_id, "POST", PATH_HEARTBEAT)
        })
        .await?;

    let row = sqlx::query!(
        r#"SELECT last_idle_seconds, worker_status, last_heartbeat_at, last_seen_at
             FROM remote_control.channels WHERE cse_id = $1"#,
        cse_id
    )
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(
        row.last_idle_seconds,
        Some(42),
        "a stale heartbeat must not overwrite a newer idle reading"
    );
    assert_eq!(
        row.worker_status.as_deref(),
        Some("requires_action"),
        "a heartbeat carrying no status must not clear the status a PUT recorded"
    );
    assert_eq!(row.last_heartbeat_at, Some(at));
    assert_eq!(row.last_seen_at, at + chrono::Duration::seconds(1));

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn a_session_delete_keeps_the_channel_and_clears_the_link() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_linked");
    let project_path = unique("/tmp/rc");

    let project_id = sqlx::query_scalar!(
        r#"INSERT INTO projects.projects (project_path, display_name)
           VALUES ($1, $1) RETURNING id AS "id!""#,
        project_path
    )
    .fetch_one(handle.pool())
    .await?;
    let session_id = sqlx::query_scalar!(
        r#"INSERT INTO sessions.sessions (session_uuid, project_id)
           VALUES (gen_random_uuid(), $1) RETURNING id AS "id!""#,
        project_id
    )
    .fetch_one(handle.pool())
    .await?;

    handle
        .record_remote_control_call(call_at(&cse_id, "POST", PATH_HEARTBEAT))
        .await?;
    sqlx::query!(
        "UPDATE remote_control.channels SET session_id = $1 WHERE cse_id = $2",
        session_id,
        cse_id
    )
    .execute(handle.pool())
    .await?;

    sqlx::query!("DELETE FROM sessions.sessions WHERE id = $1", session_id)
        .execute(handle.pool())
        .await?;

    let row = sqlx::query!(
        r#"SELECT session_id, cse_id FROM remote_control.channels WHERE cse_id = $1"#,
        cse_id
    )
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(
        row.session_id, None,
        "a swept session clears the link but must not take the channel with it"
    );

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn an_unseen_worker_status_is_captured_rather_than_rejected() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_open_set");

    handle
        .record_remote_control_call(RemoteControlCall {
            signals: RemoteControlSignals {
                worker_status: Some("awaiting_operator"),
                ..RemoteControlSignals::default()
            },
            ..call_at(&cse_id, "PUT", "/worker")
        })
        .await?;

    let status = sqlx::query_scalar!(
        r#"SELECT worker_status FROM remote_control.channels WHERE cse_id = $1"#,
        cse_id
    )
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(
        status.as_deref(),
        Some("awaiting_operator"),
        "the status set is open; an unseen value is stored, not rejected"
    );

    handle.pool().close().await;
    Ok(())
}
