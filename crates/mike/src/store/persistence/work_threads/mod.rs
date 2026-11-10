use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use super::PersistError;

mod candidates;
pub(crate) mod cards;
mod checkpoint;
mod criteria;
mod entry;
mod import;
mod lock;
mod meta;
mod project;
mod reads;
mod validate;

#[cfg(test)]
mod tests;

pub(crate) use candidates::{CandidateThread, NewThreadCandidate};
pub use checkpoint::{CheckpointInput, CheckpointOutcome, StateFields};
pub use entry::{EntryInput, EntryOutcome};
pub use import::{ImportOutcome, ImportedEntry, ImportedState, ImportedThread};
pub use meta::MetaUpdate;
pub use reads::{HistoryPage, ListFilter};
pub use validate::ALLOWED_STATUSES;

#[derive(Debug, thiserror::Error)]
pub enum WorkThreadError {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    NotFound(String),
    #[error(transparent)]
    Persist(#[from] PersistError),
}

impl From<sqlx::Error> for WorkThreadError {
    fn from(error: sqlx::Error) -> Self {
        Self::Persist(PersistError::Database(error))
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ThreadIdentity {
    pub id: i64,
    pub slug: String,
    pub title: Option<String>,
    pub goal: Option<String>,
    pub status: Option<String>,
    pub branch: Option<String>,
    pub worktree: Option<String>,
    pub last_session_uuid: Option<Uuid>,
    pub source_path: Option<String>,
    pub parse_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ThreadState {
    pub tldr: String,
    pub where_we_are: String,
    pub next_up: String,
    pub blockers: String,
    pub open_questions: String,
    pub decisions: String,
    pub files: Vec<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct HistoryEntry {
    pub ord: i32,
    pub kind: String,
    pub entry_date: NaiveDate,
    pub session_uuid: Option<Uuid>,
    pub commit_sha: Option<String>,
    pub summary: String,
    pub body: String,
    pub data: Option<Value>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct LinkedSession {
    pub session_uuid: Uuid,
    pub contribution: String,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThreadDetail {
    pub thread: ThreadIdentity,
    pub state: Option<ThreadState>,
    pub acceptance_criteria: Option<String>,
    pub depends_on: Vec<String>,
    pub depended_on_by: Vec<String>,
    pub history: Vec<HistoryEntry>,
    pub history_total: i64,
    pub history_next_before_ord: Option<i32>,
    pub sessions: Vec<LinkedSession>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ThreadListRow {
    pub slug: String,
    pub title: Option<String>,
    pub status: Option<String>,
    pub tldr: String,
    pub next_up: String,
    pub parse_error: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThreadList {
    pub threads: Vec<ThreadListRow>,
    pub total: i64,
}
