use std::sync::LazyLock;

use dashmap::DashSet;

static MAILBOX_SERVED: LazyLock<DashSet<String>> = LazyLock::new(DashSet::new);

pub fn mark_mailbox_served(session_id: &str) {
    MAILBOX_SERVED.insert(session_id.to_owned());
}

pub fn mailbox_served(session_id: &str) -> bool {
    MAILBOX_SERVED.contains(session_id)
}
