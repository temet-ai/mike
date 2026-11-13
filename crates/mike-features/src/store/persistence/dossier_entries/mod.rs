pub mod load;
pub mod write;

use std::fmt;

use chrono::{DateTime, Utc};
use serde::Serialize;

pub use load::{BackingItemRow, DirectiveSourceRow, PromiseSourceRow};
pub use write::EntryCountRow;

pub const DIRECTIVES_VERSION: &str = "directives-v1";
pub const PROMISES_VERSION: &str = "promises-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    Directive,
    Promise,
}

impl EntryKind {
    pub fn code(self) -> &'static str {
        match self {
            EntryKind::Directive => "directive",
            EntryKind::Promise => "promise",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryStatus {
    Open,
    Kept,
    Broken,
    Withdrawn,
    Superseded,
}

impl EntryStatus {
    pub fn code(self) -> &'static str {
        match self {
            EntryStatus::Open => "open",
            EntryStatus::Kept => "kept",
            EntryStatus::Broken => "broken",
            EntryStatus::Withdrawn => "withdrawn",
            EntryStatus::Superseded => "superseded",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceRef {
    pub table: &'static str,
    pub id: i64,
}

impl SourceRef {
    pub fn request(id: i64) -> Self {
        Self {
            table: "requests.requests",
            id,
        }
    }

    pub fn operator_turn(id: i64) -> Self {
        Self {
            table: "memory.operator_turns",
            id,
        }
    }
}

impl fmt::Display for SourceRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.table, self.id)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct DossierEntryRow {
    pub id: i64,
    pub session_id: i64,
    pub operator_turn_id: Option<i64>,
    pub kind: String,
    pub data: serde_json::Value,
    pub status: String,
    pub source: String,
    pub source_ref: String,
    pub dedup_key: String,
    pub extractor_version: String,
    pub effective_at: DateTime<Utc>,
    pub recorded_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub close_recorded_at: Option<DateTime<Utc>>,
    pub closed_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NewDossierEntry {
    pub session_id: i64,
    pub operator_turn_id: Option<i64>,
    pub kind: &'static str,
    pub data: serde_json::Value,
    pub source_ref: String,
    pub dedup_key: String,
    pub extractor_version: &'static str,
    pub effective_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct EntryClose {
    pub id: i64,
    pub status: String,
    pub closed_at: DateTime<Utc>,
    pub closed_by: String,
}
