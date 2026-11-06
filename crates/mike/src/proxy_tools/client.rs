use std::sync::LazyLock;

use dashmap::DashSet;

static SERVED_BY_CLIENT: LazyLock<DashSet<String>> = LazyLock::new(DashSet::new);

pub(crate) fn mark_served(session_id: &str) {
    SERVED_BY_CLIENT.insert(session_id.to_owned());
}

pub(crate) fn served_by_client(session_id: &str) -> bool {
    SERVED_BY_CLIENT.contains(session_id)
}
