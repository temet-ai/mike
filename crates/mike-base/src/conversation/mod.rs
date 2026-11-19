pub mod chain;
pub mod restore;

use std::sync::Arc;

use serde::Deserialize;
use serde_json::Value;
use serde_json::value::RawValue;

pub type Message = Arc<RawValue>;
pub type Messages = Arc<[Message]>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EarlierState {
    #[default]
    WholeBody,
    Chain,
    Partial,
    Unavailable,
}

#[derive(Debug, Clone, Default)]
pub struct EarlierTurns {
    messages: Option<Messages>,
    state: EarlierState,
}

pub static WHOLE_BODY: EarlierTurns = EarlierTurns::whole_body();

impl EarlierTurns {
    pub const fn whole_body() -> Self {
        Self {
            messages: None,
            state: EarlierState::WholeBody,
        }
    }

    pub fn unavailable() -> Self {
        Self {
            messages: None,
            state: EarlierState::Unavailable,
        }
    }

    pub fn from_chain(messages: Messages) -> Self {
        Self {
            messages: Some(messages),
            state: EarlierState::Chain,
        }
    }

    pub fn partial(messages: Messages) -> Self {
        Self {
            messages: Some(messages),
            state: EarlierState::Partial,
        }
    }

    pub fn state(&self) -> EarlierState {
        self.state
    }

    pub fn is_unavailable(&self) -> bool {
        self.state == EarlierState::Unavailable
    }

    pub fn is_complete(&self) -> bool {
        matches!(self.state, EarlierState::WholeBody | EarlierState::Chain)
    }

    pub fn len(&self) -> usize {
        self.messages.as_ref().map_or(0, |m| m.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn raw(&self) -> impl DoubleEndedIterator<Item = &RawValue> {
        self.messages
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|m| m.as_ref())
    }

    pub fn values(&self) -> Vec<Value> {
        self.raw()
            .filter_map(|m| serde_json::from_str(m.get()).ok())
            .collect()
    }

    pub fn contains_text(&self, needle: &str) -> bool {
        !needle.is_empty() && self.raw().any(|m| m.get().contains(needle))
    }

    pub fn last_with_role(&self, role: &str) -> Option<Value> {
        self.raw()
            .rev()
            .find(|m| role_of(m) == Some(role))
            .and_then(|m| serde_json::from_str(m.get()).ok())
    }

    pub fn conversation_with(&self, body_messages: &[Value]) -> Vec<Value> {
        let mut all = self.values();
        all.extend(body_messages.iter().cloned());
        all
    }
}

pub fn role_of(message: &RawValue) -> Option<&str> {
    #[derive(Deserialize)]
    struct Role<'a> {
        #[serde(borrow, default)]
        role: Option<&'a str>,
    }
    serde_json::from_str::<Role<'_>>(message.get())
        .ok()
        .and_then(|r| r.role)
}

pub fn body_messages(body: &[u8]) -> Option<Vec<Message>> {
    #[derive(Deserialize)]
    struct Body {
        messages: Vec<Box<RawValue>>,
    }
    serde_json::from_slice::<Body>(body)
        .ok()
        .map(|b| b.messages.into_iter().map(Arc::from).collect())
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
