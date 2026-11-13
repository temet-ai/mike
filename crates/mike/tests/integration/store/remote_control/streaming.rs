use chrono::{SubsecRound, Utc};
use mike::store::persistence::proxy_repo::ProxyPersist as _;

use mike::store::persistence::PersistError;
use mike::store::persistence::remote_control::{PATH_STREAM, RemoteControlCall};

use crate::store::remote_control::support::{call_at, shared_handle, unique};

#[tokio::test]
async fn a_stream_opens_then_closes_on_the_same_row() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_stream");

    let opened_at = Utc::now().trunc_subsecs(6);
    let call_id = handle
        .proxy()
        .record_remote_control_call(RemoteControlCall {
            http_status: Some(200),
            created_at: opened_at,
            completed_at: None,
            ..call_at(&cse_id, "GET", PATH_STREAM)
        })
        .await?;

    let open = sqlx::query!(
        r#"SELECT c.completed_at, ch.stream_opened_at, ch.stream_closed_at
             FROM remote_control.calls c
             JOIN remote_control.channels ch ON ch.id = c.channel_id
            WHERE c.id = $1"#,
        call_id
    )
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(open.completed_at, None, "the call is still open");
    assert_eq!(open.stream_opened_at, Some(opened_at));
    assert_eq!(open.stream_closed_at, None);

    let closed_at = opened_at + chrono::Duration::seconds(90);
    handle
        .proxy()
        .close_remote_control_stream(call_id, closed_at)
        .await?;

    let closed = sqlx::query!(
        r#"SELECT c.completed_at, ch.stream_closed_at, ch.last_seen_at
             FROM remote_control.calls c
             JOIN remote_control.channels ch ON ch.id = c.channel_id
            WHERE c.id = $1"#,
        call_id
    )
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(closed.completed_at, Some(closed_at));
    assert_eq!(closed.stream_closed_at, Some(closed_at));
    assert_eq!(closed.last_seen_at, closed_at);

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn closing_an_unknown_call_is_an_error_not_a_silent_no_op() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };

    let Err(err) = handle
        .proxy()
        .close_remote_control_stream(-999_999, Utc::now())
        .await
    else {
        anyhow::bail!("a missing call id must surface, not vanish");
    };
    assert!(
        matches!(err, PersistError::DataInvariant(ref m) if m.contains("-999999")),
        "unexpected error: {err}"
    );

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn old_stream_close_keeps_newer_stream_open() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_reconnected");
    let first = Utc::now().trunc_subsecs(6);
    let mut ids = Vec::new();
    for created_at in [first, first + chrono::Duration::seconds(1)] {
        ids.push(
            handle
                .proxy()
                .record_remote_control_call(RemoteControlCall {
                    created_at,
                    completed_at: None,
                    ..call_at(&cse_id, "GET", PATH_STREAM)
                })
                .await?,
        );
    }
    handle
        .proxy()
        .close_remote_control_stream(ids[0], first + chrono::Duration::seconds(2))
        .await?;
    let closed: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT stream_closed_at FROM remote_control.channels WHERE cse_id=$1")
            .bind(&cse_id)
            .fetch_one(handle.pool())
            .await?;
    assert!(
        closed.is_none(),
        "old connection cleanup must not close its replacement"
    );
    handle
        .proxy()
        .close_remote_control_stream(ids[1], first + chrono::Duration::seconds(3))
        .await?;
    let closed: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT stream_closed_at FROM remote_control.channels WHERE cse_id=$1")
            .bind(&cse_id)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(closed, Some(first + chrono::Duration::seconds(3)));
    handle.pool().close().await;
    Ok(())
}
