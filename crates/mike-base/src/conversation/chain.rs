use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use bytes::Bytes;
use serde_json::value::RawValue;

use super::{EarlierTurns, Message, Messages, body_messages};
use crate::store::RequestStore;
use crate::store::models::{RequestId, ThreadLink};
use crate::wire::sse_message::assistant_message_from_sse;

const MAX_CHAIN_DEPTH: usize = 256;
const MAX_HELD: usize = 64;
const MAX_REPLIES: usize = 512;

#[derive(Debug, Default)]
pub struct HeldConversations {
    held: HashMap<RequestId, Messages>,
    order: VecDeque<RequestId>,
    restoring: HashSet<RequestId>,
    by_message: HashMap<String, RequestId>,
    replies: HashMap<RequestId, Message>,
    reply_order: VecDeque<RequestId>,
}

impl HeldConversations {
    pub fn get(&self, id: &RequestId) -> Option<Messages> {
        self.held.get(id).cloned()
    }

    pub fn get_by_message(&self, upstream_message_id: &str) -> Option<Messages> {
        self.by_message
            .get(upstream_message_id)
            .and_then(|id| self.get(id))
    }

    pub fn alias(&mut self, upstream_message_id: String, id: RequestId) {
        self.by_message.insert(upstream_message_id, id);
    }

    pub fn put(&mut self, id: RequestId, messages: Messages) {
        if self.held.insert(id, messages).is_none() {
            self.order.push_back(id);
        }
        while self.order.len() > MAX_HELD {
            if let Some(oldest) = self.order.pop_front() {
                self.held.remove(&oldest);
                self.by_message.retain(|_, held_id| *held_id != oldest);
            }
        }
    }

    pub fn reply(&self, id: &RequestId) -> Option<Message> {
        self.replies.get(id).cloned()
    }

    pub fn put_reply(&mut self, id: RequestId, reply: Message) {
        if self.replies.insert(id, reply).is_none() {
            self.reply_order.push_back(id);
        }
        while self.reply_order.len() > MAX_REPLIES {
            if let Some(oldest) = self.reply_order.pop_front() {
                self.replies.remove(&oldest);
            }
        }
    }

    pub fn len(&self) -> usize {
        self.held.len()
    }

    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }
}

struct Answered {
    sent: Bytes,
    reply: Message,
    link: Option<ThreadLink>,
}

impl RequestStore {
    pub fn earlier_turns(&mut self, link: Option<&ThreadLink>) -> EarlierTurns {
        let Some(link) = link.filter(|l| l.continues()) else {
            return EarlierTurns::whole_body();
        };
        self.held_before(link, 0)
            .map_or_else(EarlierTurns::unavailable, EarlierTurns::from_chain)
    }

    fn held_before(&mut self, link: &ThreadLink, depth: usize) -> Option<Messages> {
        match link.predecessor {
            Some(predecessor) => self.held_after(predecessor, depth),
            None => self
                .held_conversations
                .get_by_message(link.previous_message_id.as_deref()?),
        }
    }

    pub fn remember_held_conversation(
        &mut self,
        id: RequestId,
        upstream_message_id: Option<String>,
        messages: Messages,
    ) {
        self.held_conversations.put(id, messages);
        if let Some(message_id) = upstream_message_id {
            self.held_conversations.alias(message_id, id);
        }
    }

    pub fn begin_restore(&mut self, id: RequestId) -> bool {
        self.held_conversations.get(&id).is_none() && self.held_conversations.restoring.insert(id)
    }

    pub fn end_restore(&mut self, id: RequestId) {
        self.held_conversations.restoring.remove(&id);
    }

    fn held_after(&mut self, id: RequestId, depth: usize) -> Option<Messages> {
        if depth > MAX_CHAIN_DEPTH {
            return None;
        }
        if let Some(held) = self.held_conversations.get(&id) {
            return Some(held);
        }
        let answered = self.answered(&id)?;
        let sent = body_messages(&answered.sent)?;
        let reply = answered.reply;
        let before: Messages = match answered.link.filter(ThreadLink::continues) {
            Some(link) => self.held_before(&link, depth + 1)?,
            None => Arc::from([]),
        };
        let held: Messages = before
            .iter()
            .cloned()
            .chain(sent)
            .chain(std::iter::once(reply))
            .collect();
        self.held_conversations.put(id, held.clone());
        Some(held)
    }

    pub fn remember_reply(&mut self, id: RequestId, reply: &serde_json::Value) {
        if let Ok(raw) = serde_json::value::to_raw_value(reply) {
            self.held_conversations.put_reply(id, Arc::from(raw));
        }
    }

    fn answered(&self, id: &RequestId) -> Option<Answered> {
        let req = self.entries.get(id)?;
        let reply = match req.response_body.as_deref() {
            Some(body) => assistant_reply(body)?,
            None => self.held_conversations.reply(id)?,
        };
        Some(Answered {
            sent: req.sent_body.clone().or_else(|| req.request_body.clone())?,
            reply,
            link: req.thread.clone(),
        })
    }
}

pub fn assistant_reply(response: &[u8]) -> Option<Message> {
    let trimmed = response.trim_ascii_start();
    if trimmed.first() == Some(&b'{') {
        let raw: Box<RawValue> = serde_json::from_slice(trimmed).ok()?;
        return Some(Arc::from(raw));
    }
    let message = assistant_message_from_sse(response)?;
    serde_json::value::to_raw_value(&message)
        .ok()
        .map(Arc::from)
}

#[cfg(test)]
#[path = "chain_tests.rs"]
mod tests;
