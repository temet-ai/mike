use chrono::Utc;

use mike::store::persistence::remote_control::{
    PATH_HEARTBEAT, RemoteControlCall, RemoteControlSignals,
};

use crate::store::remote_control::support::{call_at, shared_handle, unique};

#[tokio::test]
async fn the_first_attachment_payload_wins_the_session_link() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_link");
    let first_uuid = uuid::Uuid::new_v4().to_string();
    let second_uuid = uuid::Uuid::new_v4().to_string();

    handle
        .record_remote_control_call(RemoteControlCall {
            signals: RemoteControlSignals {
                local_session_uuid: Some(&first_uuid),
                local_cwd: Some("/repo/first"),
                client_version: Some("2.1.263"),
                ..RemoteControlSignals::default()
            },
            ..call_at(&cse_id, "POST", "/worker/events")
        })
        .await?;
    handle
        .record_remote_control_call(RemoteControlCall {
            signals: RemoteControlSignals {
                local_session_uuid: Some(&second_uuid),
                local_cwd: Some("/repo/second"),
                client_version: Some("2.1.264"),
                ..RemoteControlSignals::default()
            },
            ..call_at(&cse_id, "POST", "/worker/events")
        })
        .await?;

    let row = sqlx::query!(
        r#"SELECT local_session_uuid, local_cwd, client_version
             FROM remote_control.channels WHERE cse_id = $1"#,
        cse_id
    )
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(
        row.local_session_uuid.map(|u| u.to_string()),
        Some(first_uuid)
    );
    assert_eq!(row.local_cwd.as_deref(), Some("/repo/first"));
    assert_eq!(row.client_version.as_deref(), Some("2.1.263"));

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn a_matching_session_row_resolves_session_id() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_resolves");
    let project_path = unique("/tmp/rc-link");

    let project_id = sqlx::query_scalar!(
        r#"INSERT INTO projects.projects (project_path, display_name)
           VALUES ($1, $1) RETURNING id AS "id!""#,
        project_path
    )
    .fetch_one(handle.pool())
    .await?;
    let session_uuid = uuid::Uuid::new_v4();
    let session_id = sqlx::query_scalar!(
        r#"INSERT INTO sessions.sessions (session_uuid, project_id)
           VALUES ($1, $2) RETURNING id AS "id!""#,
        session_uuid,
        project_id
    )
    .fetch_one(handle.pool())
    .await?;

    let uuid_str = session_uuid.to_string();
    handle
        .record_remote_control_call(RemoteControlCall {
            signals: RemoteControlSignals {
                local_session_uuid: Some(&uuid_str),
                ..RemoteControlSignals::default()
            },
            ..call_at(&cse_id, "POST", "/worker/events")
        })
        .await?;

    let linked = sqlx::query_scalar!(
        r#"SELECT session_id FROM remote_control.channels WHERE cse_id = $1"#,
        cse_id
    )
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(linked, Some(session_id));

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn an_unknown_session_uuid_leaves_session_id_null() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_no_session");
    let uuid_str = uuid::Uuid::new_v4().to_string();

    handle
        .record_remote_control_call(RemoteControlCall {
            signals: RemoteControlSignals {
                local_session_uuid: Some(&uuid_str),
                ..RemoteControlSignals::default()
            },
            ..call_at(&cse_id, "POST", "/worker/events")
        })
        .await?;

    let linked = sqlx::query_scalar!(
        r#"SELECT session_id FROM remote_control.channels WHERE cse_id = $1"#,
        cse_id
    )
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(
        linked, None,
        "no sessions.sessions row exists for this uuid, so session_id stays unresolved"
    );

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn a_malformed_session_uuid_is_dropped_not_rejected() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_malformed");

    handle
        .record_remote_control_call(RemoteControlCall {
            signals: RemoteControlSignals {
                local_session_uuid: Some("not-a-uuid"),
                local_cwd: Some("/repo/odd"),
                ..RemoteControlSignals::default()
            },
            ..call_at(&cse_id, "POST", "/worker/events")
        })
        .await?;

    let row = sqlx::query!(
        r#"SELECT local_session_uuid, local_cwd FROM remote_control.channels WHERE cse_id = $1"#,
        cse_id
    )
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(row.local_session_uuid, None);
    assert_eq!(row.local_cwd.as_deref(), Some("/repo/odd"));

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn worker_status_transitions_are_appended_only_on_change() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_transitions");
    let uuid_str = uuid::Uuid::new_v4().to_string();
    let session_uuid: uuid::Uuid = uuid_str.parse()?;
    let at = Utc::now();

    for (offset, status) in [(0, "idle"), (1, "idle"), (2, "requires_action")] {
        handle
            .record_remote_control_call(RemoteControlCall {
                created_at: at + chrono::Duration::seconds(offset),
                signals: RemoteControlSignals {
                    local_session_uuid: Some(&uuid_str),
                    worker_status: Some(status),
                    ..RemoteControlSignals::default()
                },
                ..call_at(&cse_id, "PUT", "/worker")
            })
            .await?;
    }

    let count = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!"
             FROM remote_control.worker_status_transitions t
             JOIN remote_control.channels c ON c.id = t.channel_id
            WHERE c.cse_id = $1"#,
        cse_id
    )
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(
        count, 2,
        "one row for the channel's first-ever status, one for the real change to requires_action — \
         the repeated idle must not append a second row"
    );

    let history = handle.fetch_worker_status_history(session_uuid, 10).await?;
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].from_status.as_deref(), Some("idle"));
    assert_eq!(history[0].to_status, "requires_action");
    assert_eq!(history[1].from_status, None);
    assert_eq!(history[1].to_status, "idle");

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn channel_snapshot_reports_stream_open_and_post_turn_summary() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_snapshot");
    let uuid_str = uuid::Uuid::new_v4().to_string();
    let session_uuid: uuid::Uuid = uuid_str.parse()?;

    handle
        .record_remote_control_call(RemoteControlCall {
            signals: RemoteControlSignals {
                local_session_uuid: Some(&uuid_str),
                idle_seconds: Some(7),
                current_interval_seconds: Some(20),
                ..RemoteControlSignals::default()
            },
            ..call_at(&cse_id, "POST", PATH_HEARTBEAT)
        })
        .await?;
    handle
        .record_remote_control_call(RemoteControlCall {
            signals: RemoteControlSignals {
                post_turn_summary: Some(&serde_json::json!({"status_category": "completed"})),
                ..RemoteControlSignals::default()
            },
            ..call_at(&cse_id, "PUT", "/worker")
        })
        .await?;

    let stream_call_id = handle
        .record_remote_control_call(RemoteControlCall {
            completed_at: None,
            ..call_at(
                &cse_id,
                "GET",
                mike::store::persistence::remote_control::PATH_STREAM,
            )
        })
        .await?;

    let open = handle.fetch_channel_snapshots(&[session_uuid]).await?;
    assert_eq!(open.len(), 1);
    assert!(open[0].stream_open, "the stream is open, never closed yet");
    assert_eq!(open[0].last_idle_seconds, Some(7));
    assert_eq!(
        open[0].post_turn_summary,
        Some(serde_json::json!({"status_category": "completed"}))
    );

    handle
        .close_remote_control_stream(stream_call_id, Utc::now())
        .await?;

    let closed = handle.fetch_channel_snapshots(&[session_uuid]).await?;
    assert_eq!(closed.len(), 1);
    assert!(
        !closed[0].stream_open,
        "stream_closed_at now postdates stream_opened_at"
    );

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn channel_snapshots_are_scoped_to_the_requested_session_uuids() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let wanted = uuid::Uuid::new_v4();
    let other = uuid::Uuid::new_v4();
    let wanted_str = wanted.to_string();
    let other_str = other.to_string();

    for (cse_id, uuid_str) in [
        (unique("cse_wanted"), &wanted_str),
        (unique("cse_other"), &other_str),
    ] {
        handle
            .record_remote_control_call(RemoteControlCall {
                signals: RemoteControlSignals {
                    local_session_uuid: Some(uuid_str),
                    ..RemoteControlSignals::default()
                },
                ..call_at(&cse_id, "POST", "/worker/events")
            })
            .await?;
    }

    let rows = handle.fetch_channel_snapshots(&[wanted]).await?;
    assert_eq!(
        rows.len(),
        1,
        "only the requested session's channel comes back"
    );
    assert!(rows[0].cse_id.starts_with("cse_wanted"));

    handle.pool().close().await;
    Ok(())
}
