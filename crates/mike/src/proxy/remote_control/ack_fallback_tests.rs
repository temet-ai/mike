use std::sync::Arc;

use uuid::Uuid;

use crate::injection::InjectionGate;
use crate::store::models::{ReminderSource, SystemReminder};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;

use super::ack_fallback::release_if_unacked;
use super::inject::deliver_or_enqueue;
use super::inject_tests::{CSE, deps, register_autoack_stream, seed};
use super::repush::repush_pending;
use super::streams::ReplyStreams;

async fn seed_mail(handle: &PersistHandle, session: Uuid) -> anyhow::Result<Uuid> {
    let message_uuid = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO mailbox.messages(message_uuid, to_session_id, body) \
         SELECT $1, id, 'hello' FROM sessions.sessions WHERE session_uuid = $2",
    )
    .bind(message_uuid)
    .bind(session)
    .execute(handle.pool())
    .await?;
    Ok(message_uuid)
}

fn mail(message_uuid: Uuid) -> SystemReminder {
    SystemReminder {
        source: ReminderSource::SessionMailbox {
            message_uuid,
            from_session_uuid: None,
        },
        payload: "<system-reminder>mail</system-reminder>".to_string(),
    }
}

async fn injection_events(handle: &PersistHandle, message_uuid: Uuid) -> anyhow::Result<Vec<Uuid>> {
    Ok(sqlx::query_scalar(
        "SELECT event_id FROM remote_control.injections WHERE message_uuid = $1 ORDER BY created_at",
    )
    .bind(message_uuid)
    .fetch_all(handle.pool())
    .await?)
}

#[tokio::test]
async fn unacked_mail_push_is_released_and_pushed_again_once() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (session, _) = seed(&handle).await?;
    let message_uuid = seed_mail(&handle, session).await?;
    let streams = ReplyStreams::default();
    let _registration = register_autoack_stream(&streams);
    let deps = deps(handle.clone(), streams, Arc::new(InjectionGate::default()));

    assert!(deliver_or_enqueue(&deps, session, mail(message_uuid)).await);
    let first = injection_events(&handle, message_uuid).await?;
    assert_eq!(first.len(), 1);

    assert!(release_if_unacked(&deps, session, first[0], mail(message_uuid)).await);
    let (sent, attempts): (bool, i32) = sqlx::query_as(
        "SELECT sent_at IS NOT NULL, delivery_attempts FROM mailbox.messages WHERE message_uuid = $1",
    )
    .bind(message_uuid)
    .fetch_one(handle.pool())
    .await?;
    assert!(!sent, "a released row must be pushable again");
    assert_eq!(attempts, 2);

    assert_eq!(repush_pending(&deps, Some(CSE)).await, 1);
    assert_eq!(injection_events(&handle, message_uuid).await?.len(), 2);
    assert!(
        !release_if_unacked(&deps, session, first[0], mail(message_uuid)).await,
        "a stale timer from the first push must not release the second push"
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn acked_mail_push_is_never_released() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (session, _) = seed(&handle).await?;
    let message_uuid = seed_mail(&handle, session).await?;
    let streams = ReplyStreams::default();
    let _registration = register_autoack_stream(&streams);
    let deps = deps(handle.clone(), streams, Arc::new(InjectionGate::default()));

    assert!(deliver_or_enqueue(&deps, session, mail(message_uuid)).await);
    let events = injection_events(&handle, message_uuid).await?;
    sqlx::query("UPDATE remote_control.injections SET acked_at = now() WHERE event_id = $1")
        .bind(events[0])
        .execute(handle.pool())
        .await?;

    assert!(!release_if_unacked(&deps, session, events[0], mail(message_uuid)).await);
    let sent: bool = sqlx::query_scalar(
        "SELECT sent_at IS NOT NULL FROM mailbox.messages WHERE message_uuid = $1",
    )
    .bind(message_uuid)
    .fetch_one(handle.pool())
    .await?;
    assert!(sent, "an acknowledged push must keep its delivery stamp");

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
