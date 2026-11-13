use std::sync::LazyLock;

use dashmap::DashMap;

use super::Seen;

const REMEMBERED_SESSIONS_CAP: usize = 8192;

static SEEN_BY_SESSION: LazyLock<DashMap<String, Seen>> = LazyLock::new(DashMap::new);

pub fn seen_by(session_id: &str) -> Seen {
    SEEN_BY_SESSION
        .get(session_id)
        .map(|seen| *seen)
        .unwrap_or_default()
}

pub fn remember(session_id: &str, seen: Seen) {
    if seen == Seen::default() {
        SEEN_BY_SESSION.remove(session_id);
        return;
    }
    if SEEN_BY_SESSION.len() >= REMEMBERED_SESSIONS_CAP && !SEEN_BY_SESSION.contains_key(session_id)
    {
        SEEN_BY_SESSION.clear();
    }
    SEEN_BY_SESSION.insert(session_id.to_owned(), seen);
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn a_session_is_remembered_until_it_has_seen_nothing() {
        let session = "repeated-notices-memory-test";
        assert_eq!(seen_by(session), [false, false, false]);
        remember(session, [false, true, false]);
        assert_eq!(seen_by(session), [false, true, false]);
        assert_eq!(
            seen_by("repeated-notices-memory-other"),
            [false, false, false]
        );
        remember(session, Seen::default());
        assert_eq!(seen_by(session), [false, false, false]);
    }
}
