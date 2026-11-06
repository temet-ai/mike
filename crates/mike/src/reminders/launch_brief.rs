//! Deliver a spawned session's launch brief as mail from mike.
//!
//! The brief is pushed over Remote Control, never through the mike channel:
//! Claude Code frames a Remote Control push as a teammate's request to act
//! on, but a channel message as untrusted external data that it will not
//! act on (see docs/INJECTION_TRUST.md). If no Remote Control stream takes
//! it, the mail stays queued in memory and is injected on the session's next
//! eligible turn, which is also what makes it acknowledgeable.

use uuid::Uuid;

use crate::config::AppState;
use crate::store::models::{ReminderSource, SystemReminder};
use crate::store::persistence::mailbox::MailboxEnvelope;
use crate::store::persistence::workspace::QueuedLaunchBrief;

pub(crate) fn brief_reminder(to: Uuid, brief: &QueuedLaunchBrief) -> SystemReminder {
    SystemReminder {
        payload: MailboxEnvelope {
            from_session_uuid: None,
            queued_at: &brief.queued_at,
            body: &brief.body,
            to_session_uuid: to,
            message_uuid: brief.message_uuid,
            thread: None,
        }
        .render_as(Some(crate::store::persistence::mailbox::MIKE_SENDER)),
        source: ReminderSource::SessionMailbox {
            message_uuid: brief.message_uuid,
            from_session_uuid: None,
        },
    }
}

/// Push each brief mail over Remote Control, or queue it for the next
/// turn when no stream takes it. Returns how many were pushed.
pub(crate) async fn deliver_launch_briefs(
    state: &AppState,
    to: Uuid,
    briefs: &[QueuedLaunchBrief],
) -> usize {
    if state.persist().is_none() {
        return 0;
    }
    let deps = state.push_deps();
    let mut pushed = 0;
    for brief in briefs {
        let reminder = brief_reminder(to, brief);
        let sent =
            crate::proxy::remote_control::inject::try_push_via_remote_control(&deps, to, &reminder)
                .await;
        if sent {
            pushed += 1;
        } else {
            let mut store = deps.store.write().await;
            crate::injection::enqueue_if_admitted(
                &deps.gate,
                &mut store,
                &to.to_string(),
                reminder,
            );
        }
        tracing::info!(session = %to, message_uuid = %brief.message_uuid, pushed = sent,
            "launch brief mailed to the spawned session");
    }
    pushed
}

#[cfg(test)]
#[path = "launch_brief_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "launch_brief_db_tests.rs"]
mod db_tests;
