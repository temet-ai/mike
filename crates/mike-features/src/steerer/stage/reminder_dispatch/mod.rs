pub mod delivery;
pub mod mailbox;
pub mod staleness;

use crate::state::AppState;
use crate::store::models::SystemReminder;

pub use delivery::inject_or_requeue;
pub use mailbox::{MailboxClaim, claim_mailbox_reminder};
pub use staleness::{SteererGate, is_advisory, request_is_mid_turn, steerer_reminder_gate};

async fn requeue_reminder(state: &AppState, session_id: &str, reminder: SystemReminder) {
    state
        .store
        .write()
        .await
        .requeue_reminder(session_id, reminder);
}
