use crate::store::remote_control::support::{call_at, shared_handle, unique};

#[tokio::test]
async fn links_an_unlinked_channel_to_the_session_uuid() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_ABC123");

    handle
        .record_remote_control_call(call_at(&cse_id, "POST", "/worker/events"))
        .await?;

    let session_uuid = uuid::Uuid::new_v4();
    handle
        .link_channel_from_request_body(&cse_id, session_uuid)
        .await?;

    let linked = sqlx::query_scalar!(
        r#"SELECT local_session_uuid FROM remote_control.channels WHERE cse_id = $1"#,
        cse_id
    )
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(linked, Some(session_uuid));

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn a_second_call_does_not_move_an_already_linked_channel() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_first_write_wins");

    let first_uuid = uuid::Uuid::new_v4();
    let second_uuid = uuid::Uuid::new_v4();

    handle
        .record_remote_control_call(call_at(&cse_id, "POST", "/worker/events"))
        .await?;
    handle
        .link_channel_from_request_body(&cse_id, first_uuid)
        .await?;
    handle
        .link_channel_from_request_body(&cse_id, second_uuid)
        .await?;

    let linked = sqlx::query_scalar!(
        r#"SELECT local_session_uuid FROM remote_control.channels WHERE cse_id = $1"#,
        cse_id
    )
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(
        linked,
        Some(first_uuid),
        "a session does not change identity mid-run — the second write must be a no-op"
    );

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn a_cse_id_with_no_channel_row_is_a_silent_no_op() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_never_seen");

    handle
        .link_channel_from_request_body(&cse_id, uuid::Uuid::new_v4())
        .await?;

    let exists = sqlx::query_scalar!(
        r#"SELECT EXISTS (
             SELECT 1 FROM remote_control.channels WHERE cse_id = $1
           ) AS "present!""#,
        cse_id
    )
    .fetch_one(handle.pool())
    .await?;
    assert!(!exists, "linking must not fabricate a channel row");

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn resolves_session_id_when_the_session_already_exists() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_resolves_from_body");
    let project_path = unique("/tmp/rc-link-from-body");

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

    handle
        .record_remote_control_call(call_at(&cse_id, "POST", "/worker/events"))
        .await?;
    handle
        .link_channel_from_request_body(&cse_id, session_uuid)
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
