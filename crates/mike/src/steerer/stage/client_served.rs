use std::sync::LazyLock;

use dashmap::DashSet;

use crate::store::models::{ReminderSource, SystemReminder};

static MAILBOX_SERVED: LazyLock<DashSet<String>> = LazyLock::new(DashSet::new);

pub(crate) fn mark_mailbox_served(session_id: &str) {
    MAILBOX_SERVED.insert(session_id.to_owned());
}

pub(crate) fn mailbox_served(session_id: &str) -> bool {
    MAILBOX_SERVED.contains(session_id)
}

pub(super) fn split_mailbox(
    pending: Vec<SystemReminder>,
) -> (Vec<SystemReminder>, Vec<SystemReminder>) {
    pending
        .into_iter()
        .partition(|r| !matches!(r.source, ReminderSource::SessionMailbox { .. }))
}
