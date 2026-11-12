use crate::extract::request::TurnOrigin;
use crate::reminders::inject::InjectBookkeeping;
use crate::steerer::stage::client_served::mark_mailbox_served;
use crate::steerer::stage::drain;
use crate::store::models::{ReminderSource, RequestId, SystemReminder};
use crate::wire::manifest::ManifestBuilder;

use super::support::{drain_fixture, user_turn_body};

async fn queue_mail(
    handle: &crate::store::persistence::PersistHandle,
    state: &crate::state::AppState,
    session_uuid: &str,
) -> anyhow::Result<uuid::Uuid> {
    let message_uuid = uuid::Uuid::new_v4();
    handle
        .insert_message(message_uuid, session_uuid.parse()?, None, "mail body")
        .await?;
    state.store.write().await.enqueue_reminder(
        session_uuid,
        SystemReminder {
            payload: "mail body".to_owned(),
            source: ReminderSource::SessionMailbox {
                message_uuid,
                from_session_uuid: None,
            },
        },
    );
    Ok(message_uuid)
}

async fn run_drain(state: &crate::state::AppState, session_uuid: &str) -> Vec<SystemReminder> {
    let mut body = user_turn_body();
    let mut filtered_stored = None;
    let mut filter_applied = false;
    let mut manifest = ManifestBuilder::new("test".to_string());
    let mut bk = InjectBookkeeping {
        post_filter_body: &mut body,
        filtered_stored: &mut filtered_stored,
        filter_applied: &mut filter_applied,
        request_id: RequestId::next(),
    };
    drain(
        state,
        session_uuid,
        &TurnOrigin::Primary,
        "",
        &mut bk,
        &mut manifest,
    )
    .await
}

#[tokio::test]
async fn drain_skips_mailbox_for_a_session_that_claimed_and_keeps_it_queued() -> anyhow::Result<()>
{
    let Some((db, handle, pool, state, _p, _s, session_uuid, _t)) = drain_fixture().await? else {
        return Ok(());
    };
    queue_mail(&handle, &state, &session_uuid).await?;
    mark_mailbox_served(&session_uuid);
    assert!(run_drain(&state, &session_uuid).await.is_empty());
    assert_eq!(
        state
            .store
            .write()
            .await
            .peek_reminders(&session_uuid)
            .len(),
        1,
        "the held-back mail stays queued for the next claim"
    );
    pool.close().await;
    db.cleanup().await
}

#[tokio::test]
async fn drain_still_delivers_mailbox_for_a_session_that_never_claimed() -> anyhow::Result<()> {
    let Some((db, handle, pool, state, _p, _s, session_uuid, _t)) = drain_fixture().await? else {
        return Ok(());
    };
    queue_mail(&handle, &state, &session_uuid).await?;
    assert_eq!(run_drain(&state, &session_uuid).await.len(), 1);
    pool.close().await;
    db.cleanup().await
}
