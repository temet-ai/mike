mod delivery;
mod mailbox;
mod staleness;

use crate::config::AppState;
use crate::store::models::SystemReminder;

pub(super) use delivery::inject_or_requeue;
pub(super) use mailbox::{MailboxClaim, claim_mailbox_reminder};
pub(super) use staleness::{SteererGate, is_advisory, request_is_mid_turn, steerer_reminder_gate};

async fn requeue_reminder(state: &AppState, session_id: &str, reminder: SystemReminder) {
    state
        .store
        .write()
        .await
        .requeue_reminder(session_id, reminder);
}
