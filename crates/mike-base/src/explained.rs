use std::collections::HashSet;
use std::sync::LazyLock;

use dashmap::DashMap;
use dashmap::mapref::one::RefMut;
use serde::Deserialize;
use serde_json::value::RawValue;

const MAX_TRACKED_SESSIONS: usize = 4096;

pub const MEMORY_TAGS: &str = "memory_tags";
pub const GIT_READ_HINT: &str = "git_read_hint";
pub const SELF_IDENTITY: &str = "self_identity";

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

pub fn observe(session: &str, messages: usize) {
    if messages == 0 {
        return;
    }
    let mut seen = entry(session);
    if messages < seen.messages {
        seen.given.clear();
    }
    seen.messages = messages;
}

pub fn observe_body(session: &str, body: &[u8]) {
    let shape = body_shape(body);
    if !shape.continues_a_thread {
        observe(session, shape.messages);
    }
}

pub fn first_time(session: &str, what: &str) -> bool {
    entry(session).given.insert(what.to_owned())
}

pub fn new_topics(session: &str, topics: &[String]) -> Vec<String> {
    topics
        .iter()
        .filter(|topic| first_time(session, &format!("topic:{topic}")))
        .cloned()
        .collect()
}

struct BodyShape {
    messages: usize,
    continues_a_thread: bool,
}

fn body_shape(body: &[u8]) -> BodyShape {
    #[derive(Deserialize)]
    struct Thread<'a> {
        #[serde(rename = "type", borrow, default)]
        kind: Option<&'a str>,
    }
    #[derive(Deserialize)]
    struct Conversation<'a> {
        #[serde(borrow)]
        messages: Vec<&'a RawValue>,
        #[serde(borrow, default)]
        thread: Option<Thread<'a>>,
    }
    serde_json::from_slice::<Conversation<'_>>(body).map_or(
        BodyShape {
            messages: 0,
            continues_a_thread: false,
        },
        |c| BodyShape {
            messages: c.messages.len(),
            continues_a_thread: c.thread.and_then(|t| t.kind) == Some("continue"),
        },
    )
}

#[cfg(test)]
#[path = "explained_tests.rs"]
mod tests;
