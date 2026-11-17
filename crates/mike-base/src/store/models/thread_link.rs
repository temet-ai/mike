use std::borrow::Cow;

use serde::Deserialize;

use super::request_id::RequestId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadKind {
    Create,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadLink {
    pub kind: ThreadKind,
    pub previous_message_id: Option<String>,
    pub predecessor: Option<RequestId>,
    pub predecessor_instance: Option<i64>,
}

impl ThreadLink {
    pub fn continues(&self) -> bool {
        self.kind == ThreadKind::Continue
    }

    pub fn from_body(body: &[u8]) -> Option<Self> {
        let wire: Body<'_> = serde_json::from_slice(body).ok()?;
        let thread = wire.thread?;
        let kind = match thread.kind?.as_ref() {
            "create" => ThreadKind::Create,
            "continue" => ThreadKind::Continue,
            _ => return None,
        };
        Some(Self {
            kind,
            previous_message_id: thread
                .previous_message_id
                .filter(|id| !id.is_empty())
                .map(Cow::into_owned),
            predecessor: None,
            predecessor_instance: None,
        })
    }
}

#[derive(Deserialize)]
struct Body<'a> {
    #[serde(borrow, default)]
    thread: Option<Thread<'a>>,
}

#[derive(Deserialize)]
struct Thread<'a> {
    #[serde(rename = "type", borrow, default)]
    kind: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    previous_message_id: Option<Cow<'a, str>>,
}

#[cfg(test)]
#[path = "thread_link_tests.rs"]
mod tests;
