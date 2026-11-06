use mike::store::persistence::remote_control::{PATH_HEARTBEAT, RemoteControlCall};

use crate::store::remote_control::support::{call_at, digest, shared_handle, unique};

#[tokio::test]
async fn a_digest_only_body_keeps_its_size_and_digest_but_not_its_bytes() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_push");

    let push = br#"{"worker_epoch":5,"events":[{"payload":"the whole session transcript"}]}"#;
    let ack = br#"{"ok":true}"#;
    let call_id = handle
        .record_remote_control_call(RemoteControlCall {
            request_body: None,
            request_body_sha256: Some(digest(push)),
            request_body_size: Some(i32::try_from(push.len())?),
            response_body: Some(ack),
            response_body_sha256: Some(digest(ack)),
            response_body_size: Some(i32::try_from(ack.len())?),
            ..call_at(&cse_id, "POST", "/worker/events")
        })
        .await?;

    let row = sqlx::query!(
        r#"SELECT request_body, request_body_sha256, request_body_size, response_body
             FROM remote_control.calls WHERE id = $1"#,
        call_id
    )
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(row.request_body, None, "the transcript body is not stored");
    assert_eq!(row.request_body_size, Some(i32::try_from(push.len())?));
    assert_eq!(
        row.request_body_sha256.as_deref(),
        Some(digest(push).as_slice()),
        "the digest covers the whole body, so two pushes can be compared"
    );
    assert_eq!(
        row.response_body.as_deref(),
        Some(ack.as_slice()),
        "the small ack is still stored in full"
    );

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn a_stored_body_round_trips_whole() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_whole");

    let body = br#"{"session_id":"cse_whole","worker_epoch":5,"supports_heartbeat_probe":true,"current_interval_seconds":20,"idle_seconds":0}"#;
    let call_id = handle
        .record_remote_control_call(RemoteControlCall {
            request_body: Some(body),
            request_body_sha256: Some(digest(body)),
            request_body_size: Some(i32::try_from(body.len())?),
            ..call_at(&cse_id, "POST", PATH_HEARTBEAT)
        })
        .await?;

    let stored = sqlx::query_scalar!(
        r#"SELECT request_body FROM remote_control.calls WHERE id = $1"#,
        call_id
    )
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(
        stored.as_deref(),
        Some(body.as_slice()),
        "a captured body is the whole body, byte for byte"
    );

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn an_absent_body_is_distinguishable_from_an_empty_one() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_sizes");

    let read = handle
        .record_remote_control_call(call_at(&cse_id, "GET", "/worker"))
        .await?;
    let empty = handle
        .record_remote_control_call(RemoteControlCall {
            request_body: Some(b""),
            request_body_sha256: Some(digest(b"")),
            request_body_size: Some(0),
            ..call_at(&cse_id, "POST", "/bridge")
        })
        .await?;

    let sizes = sqlx::query!(
        r#"SELECT id, request_body_size FROM remote_control.calls
            WHERE id = ANY($1) ORDER BY id"#,
        &[read, empty][..]
    )
    .fetch_all(handle.pool())
    .await?;
    assert_eq!(sizes[0].request_body_size, None, "no body at all");
    assert_eq!(sizes[1].request_body_size, Some(0), "a present, empty body");

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn a_truncated_body_cannot_be_written() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_guard");

    let call_id = handle
        .record_remote_control_call(call_at(&cse_id, "POST", PATH_HEARTBEAT))
        .await?;

    let write = sqlx::query!(
        r#"UPDATE remote_control.calls
              SET request_body = '\x7b7d'::bytea, request_body_size = 200
            WHERE id = $1"#,
        call_id
    )
    .execute(handle.pool())
    .await;
    let Err(err) = write else {
        anyhow::bail!("storing a slice next to a full-size claim must fail");
    };
    assert!(
        err.to_string().contains("calls_request_body_is_whole"),
        "unexpected error: {err}"
    );

    handle.pool().close().await;
    Ok(())
}
