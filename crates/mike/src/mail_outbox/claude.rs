use async_trait::async_trait;
use uuid::Uuid;

use super::mailbox_link::{MailboxState, mailbox_state};
use super::{Delivery, Found, Reach, Recipient, Sent, Sighting, Stage};
use crate::store::persistence::PersistHandle;

pub(crate) struct ClaudeRecipient {
    pub(crate) persist: PersistHandle,
    pub(crate) session: Uuid,
}

const MAILBOX_OWNS: &str = "Claude mail is delivered by the mailbox itself (request injection, channel push or tether claim)";

#[async_trait]
impl Recipient for ClaudeRecipient {
    async fn generation(&self) -> Option<i64> {
        match self.persist.find_launch_by_session(self.session).await {
            Ok(row) => Some(row.map_or(0, |launch| launch.id)),
            Err(error) => {
                tracing::warn!(error = %error, session_id = %self.session, "mail outbox could not read the session's launch");
                None
            }
        }
    }

    async fn reach(&self) -> Reach {
        Reach::Unreachable(MAILBOX_OWNS.into())
    }

    async fn find(&self, message: Uuid) -> Found {
        match mailbox_state(self.persist.pool(), message).await {
            Ok(Some(state)) => found_in(&state),
            Ok(None) => Found::Unknown(format!("no mailbox row holds message {message}")),
            Err(error) => Found::Unknown(format!("the mailbox row could not be read: {error}")),
        }
    }

    async fn send(&self, _delivery: &Delivery<'_>) -> Sent {
        Sent::Refused(MAILBOX_OWNS.into())
    }
}

pub(crate) fn found_in(state: &MailboxState) -> Found {
    let mut stages = Vec::new();
    if let Some(at) = state.channel_delivered_at {
        stages.push((
            Stage::Accepted,
            format!("mailbox: pushed over the session's channel at {at}"),
        ));
    } else if let Some(at) = state.claimed_at {
        stages.push((
            Stage::Accepted,
            format!("mailbox: claimed for delivery at {at}"),
        ));
    }
    if let Some(request) = state.sent_request_id {
        stages.push((
            Stage::Consumed,
            format!("mailbox: injected into request {request}"),
        ));
        if state.answered {
            stages.push((
                Stage::ActionStarted,
                format!("request {request} has a response"),
            ));
        }
    } else if let Some(at) = state.consumed_at {
        stages.push((
            Stage::Consumed,
            format!("mailbox: the recipient acknowledged it at {at}"),
        ));
    }
    if stages.is_empty() {
        return Found::NotSeen;
    }
    if !stages.iter().any(|(s, _)| *s == Stage::Accepted)
        && let Some((_, implied)) = stages.first().cloned()
    {
        stages.insert(0, (Stage::Accepted, implied));
    }
    Found::Seen(Sighting {
        turn_id: None,
        stages,
    })
}
