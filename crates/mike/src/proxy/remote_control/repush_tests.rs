use std::sync::Arc;

use crate::injection::InjectionGate;
use crate::store::RequestStore;
use crate::store::models::{ReminderSource, SystemReminder};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;
use crate::store::retention::RetentionPolicy;

use super::inject_tests::{CSE, deps, register_autoack_stream, seed, steerer_reminder};
use super::repush::repush_pending;
use super::streams::ReplyStreams;

fn lesson(text: &str) -> SystemReminder {
    SystemReminder {
        source: ReminderSource::Lesson,
        payload: text.to_string(),
    }
}

#[test]
fn take_reminders_where_keeps_the_rest_in_order() {
    let mut store = RequestStore::new(RetentionPolicy::default());
    for text in ["a", "b", "c", "d"] {
        store.enqueue_reminder("s", lesson(text));
    }
    let taken = store.take_reminders_where("s", |r| r.payload == "b" || r.payload == "d");
    let taken: Vec<_> = taken.into_iter().map(|r| r.payload).collect();
    let kept: Vec<_> = store
        .peek_reminders("s")
        .into_iter()
        .map(|r| r.payload)
        .collect();
    assert_eq!(taken, ["b", "d"]);
    assert_eq!(kept, ["a", "c"]);
}

#[test]
fn sessions_with_pending_reminders_lists_only_non_empty_queues() {
    let mut store = RequestStore::new(RetentionPolicy::default());
    store.enqueue_reminder("busy", lesson("x"));
    store.enqueue_reminder("drained", lesson("y"));
    store.claim_reminders("drained");
    assert_eq!(store.sessions_with_pending_reminders(), ["busy"]);
}

#[tokio::test]
async fn repush_pending_delivers_a_reminder_queued_before_the_stream_opened() -> anyhow::Result<()>
{
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (session, intervention_uuid) = seed(&handle).await?;
    let streams = ReplyStreams::default();
    let deps = deps(
        handle.clone(),
        streams.clone(),
        Arc::new(InjectionGate::default()),
    );
    deps.store
        .write()
        .await
        .enqueue_reminder(&session.to_string(), steerer_reminder(intervention_uuid));

    assert_eq!(repush_pending(&deps, None).await, 0);
    assert_eq!(
        deps.store
            .read()
            .await
            .peek_reminders(&session.to_string())
            .len(),
        1,
        "with no stream the reminder must stay queued"
    );

    let _registration = register_autoack_stream(&streams);
    assert_eq!(repush_pending(&deps, Some(CSE)).await, 1);
    assert!(
        deps.store
            .read()
            .await
            .peek_reminders(&session.to_string())
            .is_empty(),
        "a pushed reminder must leave the queue so the request path cannot inject it again"
    );
    let emitted: bool = sqlx::query_scalar(
        "SELECT emitted_at IS NOT NULL FROM remote_control.injections i \
         JOIN remote_control.channels c ON c.id = i.channel_id WHERE c.cse_id = $1",
    )
    .bind(CSE)
    .fetch_one(handle.pool())
    .await?;
    assert!(emitted);

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn repush_pending_ignores_a_stream_for_a_different_channel() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (session, intervention_uuid) = seed(&handle).await?;
    let streams = ReplyStreams::default();
    let _registration = register_autoack_stream(&streams);
    let deps = deps(handle.clone(), streams, Arc::new(InjectionGate::default()));
    deps.store
        .write()
        .await
        .enqueue_reminder(&session.to_string(), steerer_reminder(intervention_uuid));

    assert_eq!(repush_pending(&deps, Some("cse_someone_else")).await, 0);
    assert_eq!(
        deps.store
            .read()
            .await
            .peek_reminders(&session.to_string())
            .len(),
        1
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
