use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct SessionObservationRow {
    pub session_uuid: Uuid,
    pub runtime: String,
    pub source: String,
    pub source_version: Option<String>,
    pub fidelity: String,
    pub completeness: String,
    pub observed_at: Option<DateTime<Utc>>,
    pub lifecycle: Option<String>,
    pub lifecycle_at: Option<DateTime<Utc>>,
    pub control: Option<String>,
    pub requested_model: Option<String>,
    pub observed_model: Option<String>,
    pub usage_scope: Option<String>,
    pub usage_basis: Option<String>,
    pub input_tokens: Option<i64>,
    pub cached_input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub reasoning_output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub context_window: Option<i64>,
    pub model_call_count: i64,
}
