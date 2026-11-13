use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;
use uuid::Uuid;

use super::{ThreadIdentity, ThreadState};

pub mod inputs;
pub mod related;
pub mod scan;
pub mod snapshot;
pub mod write;

pub use snapshot::{LocationRoot, OpenCardRow};

pub const DECISION_CAP: i64 = 3;
pub const NOTE_CAP: i64 = 2;
pub const PROMISE_CAP: i64 = 2;
pub const ASK_CAP: i64 = 2;
pub const RELATED_CAP: i64 = 3;
pub const RELATED_MIN_SCORE: f32 = -1.25;
pub const RELATED_MIN_PROMPT_CHARS: i32 = 40;
pub const RECENT_DAYS: i32 = 14;

#[derive(Debug, Clone)]
pub struct CardEntry {
    pub thread_id: i64,
    pub id: i64,
    pub kind: String,
    pub entry_date: NaiveDate,
    pub session_uuid: Option<Uuid>,
    pub summary: String,
}

#[derive(Debug, Clone)]
pub struct CardNote {
    pub thread_id: i64,
    pub note_uuid: Uuid,
    pub kind: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct CardPromise {
    pub thread_id: i64,
    pub id: i64,
    pub operator_turn_id: Option<i64>,
    pub text: Option<String>,
    pub effective_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct CardAsk {
    pub thread_id: i64,
    pub operator_turn_id: i64,
    pub unmet_ask: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CardRelated {
    pub thread_id: i64,
    pub reference: String,
    pub score: f32,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct CardInputs {
    pub thread: ThreadIdentity,
    pub state: Option<ThreadState>,
    pub handoff: Option<CardEntry>,
    pub decisions: Vec<CardEntry>,
    pub notes: Vec<CardNote>,
    pub promises: Vec<CardPromise>,
    pub unmet_asks: Vec<CardAsk>,
    pub related: Vec<CardRelated>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyThread {
    pub thread_id: i64,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewCard {
    pub thread_id: i64,
    pub card: Value,
    pub text: String,
    pub content_hash: String,
    pub refs: Value,
    pub source_max_at: DateTime<Utc>,
}
