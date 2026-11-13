use crate::store::persistence::proxy_repo::ProxyPersist as _;
use std::sync::Arc;

use chrono::{Duration, Utc};

use crate::injection::InjectionGate;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;

use super::inject::deliver_or_enqueue;
use super::inject_tests::{deps, register_autoack_stream, seed, steerer_reminder};
use super::streams::ReplyStreams;

async fn push_and_ack(
    handle: &PersistHandle,
    acked_ago: Duration,
) -> anyhow::Result<(uuid::Uuid, uuid::Uuid)> {
    let (session, intervention_uuid) = seed(handle).await?;
    let streams = ReplyStreams::default();
    let _registration = register_autoack_stream(&streams);
    let deps = deps(handle.clone(), streams, Arc::new(InjectionGate::default()));
    assert!(deliver_or_enqueue(&deps, session, steerer_reminder(intervention_uuid)).await);
    sqlx::query("UPDATE remote_control.injections SET acked_at = $1")
        .bind(Utc::now() - acked_ago)
        .execute(handle.pool())
        .await?;
    Ok((session, intervention_uuid))
}

async fn insert_request(handle: &PersistHandle, session: uuid::Uuid) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO requests.requests (
             session_id, project_id, method_id, path, is_streaming,
             message_count, is_tool_loop, status_id,
             params, last_user_message, human_prompt, turn_type_id,
             turn_origin_id, request_body, created_at, completed_at
         )
         SELECT s.id, s.project_id, rm.id, '/v1/messages', false,
                1, false, rs.id, '{}'::jsonb, '', '', rtt.id, rto.id,
                '{}'::bytea, NOW(), NOW()
           FROM sessions.sessions s, requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto
          WHERE s.session_uuid = $1 AND rm.code = 'POST' AND rs.code = 'complete'
            AND rtt.code = 'user_prompt' AND rto.code = 'primary'
          RETURNING id",
    )
    .bind(session)
    .fetch_one(handle.pool())
    .await?)
}

async fn delivered_request_id(
    handle: &PersistHandle,
    intervention_uuid: uuid::Uuid,
) -> anyhow::Result<Option<i64>> {
    Ok(sqlx::query_scalar(
        "SELECT delivered_request_id FROM steering.interventions WHERE uuid = $1",
    )
    .bind(intervention_uuid)
    .fetch_one(handle.pool())
    .await?)
}

#[tokio::test]
async fn first_request_after_the_ack_is_stamped_as_the_carrier() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (session, intervention_uuid) = push_and_ack(&handle, Duration::seconds(1)).await?;

    let carrier = insert_request(&handle, session).await?;
    let later = insert_request(&handle, session).await?;
    assert_eq!(
        handle
            .proxy()
            .stamp_push_carriers(session, carrier, Utc::now())
            .await?,
        1
    );
    assert_eq!(
        delivered_request_id(&handle, intervention_uuid).await?,
        Some(carrier)
    );
    assert_eq!(
        handle
            .proxy()
            .stamp_push_carriers(session, later, Utc::now())
            .await?,
        0
    );
    assert_eq!(
        delivered_request_id(&handle, intervention_uuid).await?,
        Some(carrier)
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_request_that_started_before_the_ack_is_not_the_carrier() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (session, intervention_uuid) = push_and_ack(&handle, Duration::seconds(1)).await?;

    let started_before_ack = Utc::now() - Duration::seconds(5);
    assert_eq!(
        handle
            .proxy()
            .stamp_push_carriers(
                session,
                insert_request(&handle, session).await?,
                started_before_ack
            )
            .await?,
        0
    );
    assert_eq!(
        delivered_request_id(&handle, intervention_uuid).await?,
        None
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn an_ack_older_than_the_window_is_never_stamped() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (session, intervention_uuid) = push_and_ack(&handle, Duration::minutes(30)).await?;

    assert_eq!(
        handle
            .proxy()
            .stamp_push_carriers(session, insert_request(&handle, session).await?, Utc::now())
            .await?,
        0
    );
    assert_eq!(
        delivered_request_id(&handle, intervention_uuid).await?,
        None
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
