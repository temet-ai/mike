use crate::store::persistence::proxy_repo::ProxyPersist as _;
use std::time::Duration;

use uuid::Uuid;

use crate::injection::enqueue_if_admitted;
use crate::store::models::SystemReminder;

use super::inject::PushDeps;

const MAILBOX_PUSH_ACK_GRACE: Duration = Duration::from_secs(10);
const MAILBOX_MAX_PUSH_ATTEMPTS: i32 = 5;

pub(super) fn spawn_mailbox_ack_fallback(
    deps: PushDeps,
    recipient: Uuid,
    event_id: Uuid,
    reminder: SystemReminder,
) {
    crate::observability::spans::spawn_detached(async move {
        tokio::time::sleep(MAILBOX_PUSH_ACK_GRACE).await;
        release_if_unacked(&deps, recipient, event_id, reminder).await;
    });
}

pub(super) async fn release_if_unacked(
    deps: &PushDeps,
    recipient: Uuid,
    event_id: Uuid,
    reminder: SystemReminder,
) -> bool {
    let Some(handle) = deps.persist.as_ref() else {
        return false;
    };
    match handle
        .proxy()
        .release_stale_mailbox_push(event_id, MAILBOX_MAX_PUSH_ATTEMPTS)
        .await
    {
        Ok(true) => {
            let mut store = deps.store.write().await;
            enqueue_if_admitted(&deps.gate, &mut store, &recipient.to_string(), reminder);
            log_released(recipient, event_id);
            true
        }
        Ok(false) => false,
        Err(error) => {
            log_release_failed(event_id, &error);
            false
        }
    }
}

fn log_released(recipient: Uuid, event_id: Uuid) {
    tracing::info!(
        %recipient,
        %event_id,
        "mailbox push unacked past grace window; released and requeued for another push"
    );
}

fn log_release_failed(event_id: Uuid, error: &dyn std::fmt::Display) {
    tracing::warn!(%event_id, %error, "mailbox push release-if-unacked check failed");
}
