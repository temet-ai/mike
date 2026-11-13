use uuid::Uuid;

use crate::base_state::BaseState;
use crate::store::models::{ReminderSource, SystemReminder};
use crate::store::persistence::mailbox::MailboxEnvelope;
use crate::store::persistence::workspace::QueuedLaunchBrief;

pub fn brief_reminder(to: Uuid, brief: &QueuedLaunchBrief) -> SystemReminder {
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

pub async fn deliver_launch_briefs(
    state: &BaseState,
    to: Uuid,
    briefs: &[QueuedLaunchBrief],
) -> usize {
    if state.persist().is_none() {
        return 0;
    }
    let mut pushed = 0;
    for brief in briefs {
        let reminder = brief_reminder(to, brief);
        let sent = state.mail_push().push_or_queue(to, reminder).await;
        if sent {
            pushed += 1;
        }
        tracing::info!(session_id = %to, message_uuid = %brief.message_uuid, pushed = sent,
            "launch brief mailed to the spawned session");
    }
    pushed
}

#[cfg(test)]
#[path = "brief_delivery_tests.rs"]
mod tests;
