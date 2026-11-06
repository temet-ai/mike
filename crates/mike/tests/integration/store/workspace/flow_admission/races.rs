use std::time::Duration;

use uuid::Uuid;

use mike::store::persistence::workspace::flow_admission::resolve_registration_session;

use super::input;
use crate::store::workspace::support::{shared_handle, unique};

#[tokio::test]
async fn concurrent_registrations_of_a_brand_new_session_create_exactly_one_row()
-> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let session = Uuid::new_v4();
    let cwd = unique("flow-admission-race-create");
    let (a, b) = tokio::join!(
        handle.register_workspace_session(input(session, &cwd)),
        handle.register_workspace_session(input(session, &cwd)),
    );
    let (a, b) = (a?.flow_admission, b?.flow_admission);
    assert_eq!(a.evidence, b.evidence);
    assert!(
        a.evidence.is_none(),
        "a registration call must never itself admit — only capture does"
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM sessions.sessions WHERE session_uuid = $1")
            .bind(session)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(
        count, 1,
        "concurrent registrations of the same session must not race a duplicate row"
    );
    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn capture_insert_in_flight_blocks_a_concurrent_registration_until_commit()
-> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let session = Uuid::new_v4();
    let cwd = unique("flow-admission-race-capture");
    let mut capture = handle.pool().begin().await?;
    mike::store::persistence::id_resolvers::resolve_session_id(
        &mut *capture,
        &session.to_string(),
        None,
        Some(1),
    )
    .await?;
    let other = handle.clone();
    let mut registration =
        tokio::spawn(async move { other.register_workspace_session(input(session, &cwd)).await });
    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut registration)
            .await
            .is_err(),
        "registration must block on the uncommitted capture row, not race past it"
    );
    capture.commit().await?;
    let result = registration.await??.flow_admission;
    let evidence = result
        .evidence
        .ok_or_else(|| anyhow::anyhow!("capture-admitted row must carry evidence"))?;
    assert_eq!(evidence.producer, "mike");
    assert_eq!(evidence.source, "capture");
    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn registration_insert_is_invisible_until_commit_and_a_later_capture_never_backfills_it()
-> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let session = Uuid::new_v4();
    let cwd = unique("flow-admission-race-invisible");
    let mut registration = handle.pool().begin().await?;
    let (id, admission) =
        resolve_registration_session(&mut registration, &input(session, &cwd)).await?;
    assert!(admission.evidence.is_none());
    let visible: Option<i64> =
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1")
            .bind(session)
            .fetch_optional(handle.pool())
            .await?;
    assert!(
        visible.is_none(),
        "the uncommitted registration row must not be visible to another connection"
    );
    registration.commit().await?;

    let captured = mike::store::persistence::id_resolvers::resolve_session_id(
        handle.pool(),
        &session.to_string(),
        None,
        Some(1),
    )
    .await?;
    assert_eq!(id, captured);

    let retry = handle
        .register_workspace_session(input(session, &cwd))
        .await?
        .flow_admission;
    assert!(
        retry.evidence.is_none(),
        "the row already existed before capture ever saw this session_uuid, so capture's \
         ON CONFLICT DO NOTHING must not retroactively backfill evidence — this session is a \
         candidate for the backfill subcommand, not real-time capture stamping"
    );
    handle.pool().close().await;
    Ok(())
}
