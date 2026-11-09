use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;
use uuid::Uuid;

use super::{ThreadIdentity, ThreadState};

mod inputs;
mod scan;
mod snapshot;
mod write;

pub(crate) use snapshot::{LocationRoot, OpenCardRow};

pub(crate) const DECISION_CAP: i64 = 3;
pub(crate) const NOTE_CAP: i64 = 2;
pub(crate) const PROMISE_CAP: i64 = 2;
pub(crate) const ASK_CAP: i64 = 2;
pub(crate) const RECENT_DAYS: i32 = 14;

#[derive(Debug, Clone)]
pub(crate) struct CardEntry {
    pub(crate) thread_id: i64,
    pub(crate) id: i64,
    pub(crate) kind: String,
    pub(crate) entry_date: NaiveDate,
    pub(crate) session_uuid: Option<Uuid>,
    pub(crate) summary: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CardNote {
    pub(crate) thread_id: i64,
    pub(crate) note_uuid: Uuid,
    pub(crate) kind: String,
    pub(crate) title: String,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub(crate) struct CardPromise {
    pub(crate) thread_id: i64,
    pub(crate) id: i64,
    pub(crate) operator_turn_id: Option<i64>,
    pub(crate) text: Option<String>,
    pub(crate) effective_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub(crate) struct CardAsk {
    pub(crate) thread_id: i64,
    pub(crate) operator_turn_id: i64,
    pub(crate) unmet_ask: String,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub(crate) struct CardInputs {
    pub(crate) thread: ThreadIdentity,
    pub(crate) state: Option<ThreadState>,
    pub(crate) handoff: Option<CardEntry>,
    pub(crate) decisions: Vec<CardEntry>,
    pub(crate) notes: Vec<CardNote>,
    pub(crate) promises: Vec<CardPromise>,
    pub(crate) unmet_asks: Vec<CardAsk>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DirtyThread {
    pub(crate) thread_id: i64,
    pub(crate) observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NewCard {
    pub(crate) thread_id: i64,
    pub(crate) card: Value,
    pub(crate) text: String,
    pub(crate) content_hash: String,
    pub(crate) refs: Value,
    pub(crate) source_max_at: DateTime<Utc>,
}
