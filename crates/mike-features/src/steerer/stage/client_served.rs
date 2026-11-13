pub use crate::mailbox_served::{mailbox_served, mark_mailbox_served};

use crate::store::models::{ReminderSource, SystemReminder};

pub fn split_mailbox(pending: Vec<SystemReminder>) -> (Vec<SystemReminder>, Vec<SystemReminder>) {
    pending
        .into_iter()
        .partition(|r| !matches!(r.source, ReminderSource::SessionMailbox { .. }))
}
