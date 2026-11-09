use std::sync::LazyLock;

use dashmap::DashSet;

static SERVED: LazyLock<DashSet<String>> = LazyLock::new(DashSet::new);

pub(crate) fn mark_served(session: &str) {
    SERVED.insert(session.to_owned());
}

pub(crate) fn served_by_client(session: &str) -> bool {
    SERVED.contains(session)
}
