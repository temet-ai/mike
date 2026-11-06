use std::collections::HashSet;
use std::sync::LazyLock;

use dashmap::DashMap;
use dashmap::mapref::one::RefMut;
use serde::Deserialize;
use serde_json::value::RawValue;

const MAX_TRACKED_SESSIONS: usize = 4096;

pub(crate) const MEMORY_TAGS: &str = "memory_tags";
pub(crate) const GIT_READ_HINT: &str = "git_read_hint";
pub(crate) const SELF_IDENTITY: &str = "self_identity";

#[derive(Default)]
struct Explained {
    messages: usize,
    given: HashSet<String>,
}

static SESSIONS: LazyLock<DashMap<String, Explained>> = LazyLock::new(DashMap::new);

fn entry(session: &str) -> RefMut<'static, String, Explained> {
    if SESSIONS.len() >= MAX_TRACKED_SESSIONS && !SESSIONS.contains_key(session) {
        SESSIONS.clear();
    }
    SESSIONS.entry(session.to_owned()).or_default()
}

pub(crate) fn observe(session: &str, messages: usize) {
    if messages == 0 {
        return;
    }
    let mut seen = entry(session);
    if messages < seen.messages {
        seen.given.clear();
    }
    seen.messages = messages;
}

pub(crate) fn observe_body(session: &str, body: &[u8]) {
    observe(session, message_count(body));
}

pub(crate) fn first_time(session: &str, what: &str) -> bool {
    entry(session).given.insert(what.to_owned())
}

pub(crate) fn new_topics(session: &str, topics: &[String]) -> Vec<String> {
    topics
        .iter()
        .filter(|topic| first_time(session, &format!("topic:{topic}")))
        .cloned()
        .collect()
}

fn message_count(body: &[u8]) -> usize {
    #[derive(Deserialize)]
    struct Conversation<'a> {
        #[serde(borrow)]
        messages: Vec<&'a RawValue>,
    }
    serde_json::from_slice::<Conversation<'_>>(body).map_or(0, |c| c.messages.len())
}

#[cfg(test)]
#[path = "explained_tests.rs"]
mod tests;
