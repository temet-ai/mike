pub mod read;
pub mod write;

use chrono::{DateTime, Utc};
use serde::Serialize;

pub use read::{ChangeLogFilter, ConfigSnapshot, SnapshotBuild, SnapshotEntry};
pub use write::{NewChange, append_change_in};
pub use write::{own_change_ids, set_change_source};

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct ChangeLogRow {
    pub id: i64,
    pub changed_at: DateTime<Utc>,
    pub xact_id: String,
    pub subject_kind: String,
    pub subject_key: String,
    pub op: String,
    pub old_value: Option<serde_json::Value>,
    pub new_value: Option<serde_json::Value>,
    pub redacted: bool,
    pub via: String,
    pub actor: Option<String>,
    pub request_id: Option<String>,
    pub build_sha: Option<String>,
    pub db_user: String,
    pub application_name: Option<String>,
    pub detail: serde_json::Value,
}

pub const CHANGE_LOG_COLUMNS: &str = "id, changed_at, xact_id::text AS xact_id, subject_kind, \
     subject_key, op, old_value, new_value, redacted, via, actor, request_id, build_sha, db_user, \
     application_name, detail";
