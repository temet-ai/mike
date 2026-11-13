pub mod holdout;
pub mod holdout_resplit;
pub mod import;
pub mod read;
pub mod write;
pub mod write_follow;

use chrono::{DateTime, Utc};
use serde::Serialize;

pub use holdout::{HoldoutOutcome, WithdrawReport};
pub use holdout_resplit::SplitConflictPolicy;
pub use import::{ImportOutcome, ResolvedTurn};
pub use read::{LabelQuery, VocabularyRow};
pub use write::assign_turn_split;
pub use write::{GoldEntry, GoldWrite, LabelWriteError, NewRelayed, Subject};

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct EvalLabelRow {
    pub id: i64,
    pub subject_kind: String,
    pub subject_id: i64,
    pub operator_turn_id: i64,
    pub session_id: i64,
    pub session_uuid: uuid::Uuid,
    pub axis: String,
    pub label: String,
    pub action: String,
    pub tier: String,
    pub method: String,
    pub split: String,
    pub tags: Vec<String>,
    pub severity: Option<String>,
    pub confidence: Option<String>,
    pub evidence: Option<String>,
    pub note: Option<String>,
    pub source: String,
    pub provenance: serde_json::Value,
    pub inclusion_prob: Option<f64>,
    pub verdict_snapshot: Option<serde_json::Value>,
    pub verdict_bundle_id: Option<i64>,
    pub import_batch_id: Option<i64>,
    pub supersedes_id: Option<i64>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct EvalLabelListRow {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub label: EvalLabelRow,
    pub split_reason: Option<String>,
    pub is_current: bool,
    pub subject_present: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct ImportBatchRow {
    pub id: i64,
    pub source_key: String,
    pub manifest: serde_json::Value,
    pub counts: serde_json::Value,
    pub imported_at: DateTime<Utc>,
    pub imported_by: String,
}
