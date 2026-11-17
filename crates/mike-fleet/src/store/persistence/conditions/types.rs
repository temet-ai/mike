use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::store::persistence::PersistError;

#[derive(Debug, Clone, Serialize)]
pub struct ConditionRow {
    pub id: i64,
    pub condition_uuid: Uuid,
    pub created_at: DateTime<Utc>,
    pub machine_uuid: Uuid,
    pub created_by_session: Option<Uuid>,
    pub project_id: Option<i64>,
    pub thread_slug: Option<String>,
    pub kind: String,
    pub spec: Value,
    pub not_before: DateTime<Utc>,
    pub deadline: Option<DateTime<Utc>>,
    pub check_every_secs: i32,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub action: String,
    pub target_session: Option<Uuid>,
    pub folder: Option<String>,
    pub brief: String,
    pub state: String,
    pub fired_at: Option<DateTime<Utc>>,
    pub outcome: Option<Value>,
    pub attempts: i32,
    pub next_attempt_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewCondition {
    pub machine_uuid: Uuid,
    pub created_by_session: Option<Uuid>,
    pub project_id: Option<i64>,
    pub thread_slug: Option<String>,
    pub kind: String,
    pub spec: Value,
    pub not_before: DateTime<Utc>,
    pub deadline: Option<DateTime<Utc>>,
    pub check_every_secs: i32,
    pub action: String,
    pub target_session: Option<Uuid>,
    pub folder: Option<String>,
    pub brief: String,
}

#[derive(Debug, Clone, Default)]
pub struct ConditionFilter {
    pub thread_slug: Option<String>,
    pub state: Option<String>,
    pub created_by_session: Option<Uuid>,
    pub limit: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fired {
    Held,
    Missed,
}

impl Fired {
    pub fn state(self) -> &'static str {
        match self {
            Self::Held => "fired",
            Self::Missed => "missed",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionPresence {
    pub session_uuid: Uuid,
    pub project_id: i64,
    pub last_request_at: DateTime<Utc>,
    pub cwd: Option<String>,
    pub live: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ConditionError {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Persist(#[from] PersistError),
}

impl From<sqlx::Error> for ConditionError {
    fn from(error: sqlx::Error) -> Self {
        Self::Persist(PersistError::Database(error))
    }
}
