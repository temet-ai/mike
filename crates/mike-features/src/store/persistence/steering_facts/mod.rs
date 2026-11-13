pub mod backing;
pub mod calls;
pub mod coordinator;
pub mod write;

use chrono::{DateTime, Utc};
use serde::Serialize;

pub use calls::StaleFilter;
pub use write::SteeringFactsWrite;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FactKey {
    pub session_id: i64,
    pub turn_id: i64,
    pub as_of: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct BackingRow {
    pub turn_id: i64,
    pub source: String,
    pub id: Option<i64>,
    pub kind: Option<String>,
    pub state: Option<String>,
    pub launched_at: Option<DateTime<Utc>>,
    pub due_at: Option<DateTime<Utc>>,
    pub owner_agent_id: Option<String>,
    pub peer_session_uuid: Option<String>,
    pub message_uuid: Option<String>,
    pub directive_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct CallRow {
    pub turn_id: i64,
    pub use_id: i64,
    pub at: DateTime<Utc>,
    pub tool_name: String,
    pub input: serde_json::Value,
    pub has_result: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct PersistedSteeringFacts {
    pub operator_turn_id: i64,
    pub wait_backing: Option<String>,
    pub wait_backing_refs: serde_json::Value,
    pub observed_after_change: Option<bool>,
    pub last_change_use_id: Option<i64>,
    pub last_change_at: Option<DateTime<Utc>>,
    pub last_observation_use_id: Option<i64>,
    pub last_observation_at: Option<DateTime<Utc>>,
    pub observation_kind: Option<String>,
    pub steering_facts_version: i16,
    pub session_class: String,
    pub session_class_rule: String,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct StaleFactTurn {
    pub turn_id: i64,
    pub session_id: i64,
    pub closed_at: DateTime<Utc>,
}
