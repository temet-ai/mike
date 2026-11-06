mod backfill;
mod candidates;
mod checks_load;
mod classify_load;
mod load;
mod text_facts;
mod write;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub use candidates::CandidateWindow;
pub use checks_load::BashUseRecord;
pub use classify_load::{FinalText, PromptToClassify};
pub use load::{SessionRecord, TurnRecord};
pub use text_facts::{StopFacts, TextFacts};
pub use write::{OpenedByRow, UpsertOutcome};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct TurnFactsRow {
    pub operator_turn_id: i64,
    pub session_id: i64,
    pub builder_version: String,
    pub built_at: DateTime<Utc>,
    pub max_request_id: i64,
    pub prompt_kind: String,
    pub prompt_rule: Option<String>,
    pub prompt_kind_at_close: String,
    pub prompt_rule_at_close: Option<String>,
    pub next_state: String,
    pub next_genuine_request_id: Option<i64>,
    pub next_genuine_turn_id: Option<i64>,
    pub next_genuine_at: Option<DateTime<Utc>>,
    pub next_state_at_close: String,
    pub next_genuine_request_id_at_close: Option<i64>,
    pub skipped_before_next: serde_json::Value,
    pub reply_window_segment_ids: Vec<i64>,
    pub reclassified_at: Option<DateTime<Utc>>,
    pub text_chars: i32,
    pub text_cut_at: i32,
    pub last_text_origin: Option<String>,
    pub text_spans: serde_json::Value,
    pub stop_request_id: Option<i64>,
    pub stop_reason: Option<String>,
    pub aborted: bool,
    pub interrupted: bool,
    pub close_cause: String,
    pub checks_run: serde_json::Value,
    pub checks_count: i32,
    pub checks_failed: i32,
    pub ended_on: String,
    pub ended_on_version: String,
    pub session_class: String,
    pub session_class_rule: String,
}
