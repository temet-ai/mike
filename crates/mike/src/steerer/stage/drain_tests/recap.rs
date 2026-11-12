use bytes::Bytes;

use crate::config::{AppState, ProxyConfig};
use crate::extract::request::TurnOrigin;
use crate::reminders::inject::InjectBookkeeping;
use crate::steerer::stage::drain;
use crate::store::models::{ReminderSource, RequestId, SystemReminder};
use crate::wire::manifest::ManifestBuilder;

fn mailbox_reminder() -> SystemReminder {
    SystemReminder {
        source: ReminderSource::SessionMailbox {
            message_uuid: uuid::Uuid::new_v4(),
            from_session_uuid: None,
        },
        payload: "<system-reminder>mail</system-reminder>".to_string(),
    }
}

#[tokio::test]
async fn reminder_dispatch_mailbox_recap_call_is_not_attached_and_stays_queued()
-> anyhow::Result<()> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    let session = uuid::Uuid::new_v4().to_string();
    state
        .store
        .write()
        .await
        .enqueue_reminder(&session, mailbox_reminder());

    let original = Bytes::from(serde_json::to_vec(&serde_json::json!({
        "messages": [{
            "role": "user",
            "content": "The user stepped away and is coming back. Recap in under 40 words: what is going on?"
        }]
    }))?);
    let mut body = original.clone();
    let mut filtered_stored = None;
    let mut filter_applied = false;
    let mut manifest = ManifestBuilder::new("test".to_string());
    let mut bk = InjectBookkeeping {
        post_filter_body: &mut body,
        filtered_stored: &mut filtered_stored,
        filter_applied: &mut filter_applied,
        request_id: RequestId::next(),
    };
    let injected = drain(
        &state,
        &session,
        &TurnOrigin::Primary,
        "",
        &mut bk,
        &mut manifest,
    )
    .await;

    assert!(injected.is_empty(), "recap call must receive no mail");
    assert_eq!(body, original, "recap request body must stay unchanged");
    let still_queued = state.store.read().await.peek_reminders(&session);
    assert_eq!(still_queued.len(), 1, "mail must remain queued");
    Ok(())
}
