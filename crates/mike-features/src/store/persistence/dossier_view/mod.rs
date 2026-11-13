pub mod goals;
pub mod labels;
pub mod ledger;
pub mod open_turn;
pub mod stream;
pub mod turns;
pub mod verdicts;

pub use labels::CorrectionLabelRow;
pub use turns::TurnLocator;
pub use verdicts::LiveVerdictRow;

use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct SessionHeaderRow {
    pub session_db_id: i64,
    pub session_uuid: String,
    pub project_path: Option<String>,
    pub first_seen: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct DossierStreamRow {
    pub request_id: i64,
    pub at: DateTime<Utc>,
    pub text: String,
    pub kind_code: Option<String>,
    pub rule: Option<String>,
    pub repeat_key: Option<String>,
    pub classifier_version: Option<String>,
    pub turn_id: Option<i64>,
    pub opened_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct DossierTurnRow {
    pub turn_id: i64,
    pub start_request_id: i64,
    pub created_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub opened_by: Option<String>,
    pub prompt: String,
    pub final_text: String,
    pub files_touched_count: i32,
    pub text_chars: Option<i32>,
    pub text_cut_at: Option<i32>,
    pub text_spans: Option<serde_json::Value>,
    pub last_text_origin: Option<String>,
    pub stop_request_id: Option<i64>,
    pub stop_reason: Option<String>,
    pub aborted: Option<bool>,
    pub interrupted: Option<bool>,
    pub close_cause: Option<String>,
    pub checks_run: Option<serde_json::Value>,
    pub checks_count: Option<i32>,
    pub checks_failed: Option<i32>,
    pub ended_on: Option<String>,
    pub ended_on_version: Option<String>,
    pub builder_version: Option<String>,
    pub built_at: Option<DateTime<Utc>>,
    pub max_request_id: Option<i64>,
    pub session_class: Option<String>,
    pub session_class_rule: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct LedgerAsOfRow {
    pub entry_id: i64,
    pub request_id: Option<i64>,
    pub operator_turn_id: Option<i64>,
    pub kind: String,
    pub kind_source: String,
    pub keyword_kind: Option<String>,
    pub kind_updated_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl LedgerAsOfRow {
    pub fn kind_at(&self, at: DateTime<Utc>) -> &str {
        match self.kind_updated_at {
            Some(updated) if updated <= at => &self.kind,
            Some(_) => self.keyword_kind.as_deref().unwrap_or(&self.kind),
            None => &self.kind,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct GoalsRow {
    pub goal: String,
    pub instructions: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct OpenTextRow {
    pub request_id: i64,
    pub seq_ord: i32,
    pub origin: String,
    pub content: String,
}
