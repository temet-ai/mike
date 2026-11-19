use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use serde_json::value::RawValue;

use super::{EarlierTurns, Message, Messages, body_messages};
use crate::store::RequestStore;
use crate::store::models::{RequestId, ThreadKind, ThreadLink};
use crate::wire::sse_message::assistant_message_from_sse;

const MAX_CHAIN_DEPTH: usize = 256;
const MAX_HELD: usize = 64;
const MAX_TURNS: usize = 4096;
const MAX_TURN_BYTES: usize = 256 * 1024 * 1024;
const RESTORE_RETRY_AFTER: Duration = Duration::from_secs(600);

#[derive(Debug, Clone)]
pub struct Held {
    pub messages: Messages,
    pub complete: bool,
}

impl Held {
    fn gap() -> Self {
        Self {
            messages: Arc::from([]),
            complete: false,
        }
    }
}

#[derive(Debug, Default, Clone)]
struct Turn {
    sent: Option<Bytes>,
    link: Option<ThreadLink>,
    reply: Option<Message>,
}

impl Turn {
    fn bytes(&self) -> usize {
        self.sent.as_ref().map_or(0, Bytes::len) + self.reply.as_ref().map_or(0, |r| r.get().len())
    }
}

#[derive(Debug, Default)]
pub struct HeldConversations {
    held: HashMap<RequestId, Held>,
    order: VecDeque<RequestId>,
    restoring: HashSet<RequestId>,
    failed: HashMap<RequestId, Instant>,
    by_message: HashMap<String, RequestId>,
    turns: HashMap<RequestId, Turn>,
    turn_order: VecDeque<RequestId>,
    turn_bytes: usize,
    turn_by_message: HashMap<String, RequestId>,
}

impl HeldConversations {
    pub fn get(&self, id: &RequestId) -> Option<Held> {
        self.held.get(id).cloned()
    }

    pub fn get_by_message(&self, upstream_message_id: &str) -> Option<Held> {
        self.by_message
            .get(upstream_message_id)
            .and_then(|id| self.get(id))
    }

    pub fn alias(&mut self, upstream_message_id: String, id: RequestId) {
        self.by_message.insert(upstream_message_id, id);
    }

    pub fn put(&mut self, id: RequestId, held: Held) {
        if self.held.insert(id, held).is_none() {
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
        self.turns.get(id).and_then(|turn| turn.reply.clone())
    }

    fn turn_for_message(&self, upstream_message_id: &str) -> Option<RequestId> {
        self.turn_by_message.get(upstream_message_id).copied()
    }

    fn evict_turns(&mut self) {
        while self.turn_order.len() > MAX_TURNS || self.turn_bytes > MAX_TURN_BYTES {
            let Some(oldest) = self.turn_order.pop_front() else {
                break;
            };
            if let Some(turn) = self.turns.remove(&oldest) {
                self.turn_bytes = self.turn_bytes.saturating_sub(turn.bytes());
            }
            self.turn_by_message.retain(|_, turn_id| *turn_id != oldest);
        }
    }

    fn update_turn(&mut self, id: RequestId, change: impl FnOnce(&mut Turn)) {
        if !self.turns.contains_key(&id) {
            self.turn_order.push_back(id);
        }
        let turn = self.turns.entry(id).or_default();
        let before = turn.bytes();
        change(turn);
        let after = turn.bytes();
        self.turn_bytes = self.turn_bytes.saturating_sub(before) + after;
        self.evict_turns();
    }

    pub fn put_sent(&mut self, id: RequestId, sent: Bytes, link: Option<ThreadLink>) {
        self.update_turn(id, |turn| {
            turn.sent = Some(sent);
            turn.link = link;
        });
    }

    pub fn put_reply(
        &mut self,
        id: RequestId,
        reply: Message,
        upstream_message_id: Option<String>,
    ) {
        self.update_turn(id, |turn| turn.reply = Some(reply));
        if let Some(message_id) = upstream_message_id
            && self.turns.contains_key(&id)
        {
            self.turn_by_message.insert(message_id, id);
        }
    }

    pub fn turn_bytes(&self) -> usize {
        self.turn_bytes
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
        let Some(link) = link else {
            return EarlierTurns::whole_body();
        };
        match link.kind {
            ThreadKind::Create => EarlierTurns::whole_body(),
            ThreadKind::Other => EarlierTurns::unavailable(),
            ThreadKind::Continue => match self.held_before(link, 0) {
                Some(held) if held.complete => EarlierTurns::from_chain(held.messages),
                Some(held) => EarlierTurns::partial(held.messages),
                None => EarlierTurns::unavailable(),
            },
        }
    }

    fn held_before(&mut self, link: &ThreadLink, depth: usize) -> Option<Held> {
        if let Some(predecessor) = link.predecessor {
            return self.held_after(predecessor, depth);
        }
        let message_id = link.previous_message_id.as_deref()?;
        if let Some(held) = self.held_conversations.get_by_message(message_id) {
            return Some(held);
        }
        let predecessor = self.held_conversations.turn_for_message(message_id)?;
        self.held_after(predecessor, depth)
    }

    pub fn remember_sent(&mut self, req: &crate::store::models::InterceptedRequest) {
        if req.thread.is_none() {
            return;
        }
        if let Some(sent) = req.sent_body.clone().or_else(|| req.request_body.clone()) {
            self.held_conversations
                .put_sent(req.id, sent, req.thread.clone());
        }
    }

    pub fn remember_held_conversation(
        &mut self,
        id: RequestId,
        upstream_message_id: Option<String>,
        held: Held,
    ) {
        self.held_conversations.put(id, held);
        if let Some(message_id) = upstream_message_id {
            self.held_conversations.alias(message_id, id);
        }
    }

    pub fn begin_restore(&mut self, id: RequestId) -> bool {
        let recently_failed = self
            .held_conversations
            .failed
            .get(&id)
            .is_some_and(|at| at.elapsed() < RESTORE_RETRY_AFTER);
        !recently_failed
            && self.held_conversations.get(&id).is_none()
            && self.held_conversations.restoring.insert(id)
    }

    pub fn end_restore(&mut self, id: RequestId) {
        self.held_conversations.restoring.remove(&id);
    }

    pub fn restore_failed(&mut self, id: RequestId) {
        let failed = &mut self.held_conversations.failed;
        failed.retain(|_, at| at.elapsed() < RESTORE_RETRY_AFTER);
        failed.insert(id, Instant::now());
    }

    fn held_after(&mut self, id: RequestId, depth: usize) -> Option<Held> {
        if depth > MAX_CHAIN_DEPTH {
            return Some(Held::gap());
        }
        if let Some(held) = self.held_conversations.get(&id) {
            return Some(held);
        }
        let answered = self.answered(&id)?;
        let sent = body_messages(&answered.sent)?;
        let before = match answered.link {
            Some(link) if link.kind == ThreadKind::Continue => {
                self.held_before(&link, depth + 1).unwrap_or_else(Held::gap)
            }
            Some(link) if link.kind == ThreadKind::Other => Held::gap(),
            _ => Held {
                messages: Arc::from([]),
                complete: true,
            },
        };
        let held = Held {
            messages: before
                .messages
                .iter()
                .cloned()
                .chain(sent)
                .chain(std::iter::once(answered.reply))
                .collect(),
            complete: before.complete,
        };
        if held.complete {
            self.held_conversations.put(id, held.clone());
        }
        Some(held)
    }

    pub fn remember_reply(&mut self, id: RequestId, reply: &serde_json::Value) {
        let message_id = reply
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        if let Ok(raw) = serde_json::value::to_raw_value(reply) {
            self.held_conversations
                .put_reply(id, Arc::from(raw), message_id);
        }
    }

    pub fn remember_reply_body(&mut self, id: RequestId, response_body: &[u8]) {
        if let Ok(reply) = serde_json::from_slice::<serde_json::Value>(response_body) {
            self.remember_reply(id, &reply);
        }
    }

    fn answered(&self, id: &RequestId) -> Option<Answered> {
        let Some(req) = self.entries.get(id) else {
            let turn = self.held_conversations.turns.get(id)?.clone();
            return Some(Answered {
                sent: turn.sent?,
                reply: turn.reply?,
                link: turn.link,
            });
        };
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

#[cfg(test)]
#[path = "chain_gap_tests.rs"]
mod gap_tests;
