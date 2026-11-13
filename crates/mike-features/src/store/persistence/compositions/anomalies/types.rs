use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

#[derive(sqlx::FromRow)]
pub struct AnomalyRow {
    pub classification: Option<String>,
    pub prompt_sha256: Option<String>,
    pub unknown_blocks: Option<Value>,
    pub block_text: Option<String>,
    pub occurrence_count: Option<i64>,
    pub first_seen: Option<DateTime<Utc>>,
    pub last_seen: Option<DateTime<Utc>>,
    pub observations: sqlx::types::Json<Vec<AnomalyObservation>>,
    pub group_firing_rule_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnomalyObservation {
    pub request_id: i64,
    pub session_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PromptClassModelCount {
    pub model: Option<String>,
    pub count: i64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PromptClassScope<'a> {
    pub model: Option<&'a str>,
    pub project_path: Option<&'a str>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PromptClassRow {
    pub lane: String,
    pub subagent_persona: Option<String>,
    pub observation_count: i64,
    pub first_seen: Option<DateTime<Utc>>,
    pub last_seen: Option<DateTime<Utc>>,
    pub block_count: i32,
    pub model_breakdown: sqlx::types::Json<Vec<PromptClassModelCount>>,
    pub composition_id: Option<i64>,
    pub label: Option<String>,
    pub mode: Option<String>,
    pub agent_definition_path: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PromptClassDetailRow {
    pub lane: String,
    pub subagent_persona: Option<String>,
    pub observation_count: i64,
    pub first_seen: Option<DateTime<Utc>>,
    pub last_seen: Option<DateTime<Utc>>,
    pub sample_blocks: Option<Value>,
    pub model_breakdown: sqlx::types::Json<Vec<PromptClassModelCount>>,
    pub composition_id: Option<i64>,
    pub label: Option<String>,
    pub mode: Option<String>,
    pub agent_definition_path: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PromptClassObservationRow {
    pub request_id: i64,
    pub session_id: i64,
    pub created_at: DateTime<Utc>,
    pub content_sha256: Option<String>,
    pub altered: bool,
    pub model: Option<String>,
    pub total_count: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PromptClassInstructionsRow {
    pub instructions: Option<String>,
    pub observation_count: i64,
    pub last_seen: DateTime<Utc>,
    pub latest_request_id: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PromptClassVersionRow {
    pub release: Option<String>,
    pub builds: i64,
    pub release_observation_count: i64,
    pub release_first_seen: DateTime<Utc>,
    pub release_last_seen: DateTime<Utc>,
    pub model: Option<String>,
    pub model_observation_count: i64,
    pub model_first_seen: DateTime<Utc>,
    pub model_last_seen: DateTime<Utc>,
    pub latest_request_id: i64,
    pub latest_session_id: i64,
}

#[derive(Debug)]
pub struct AnomalyGroup {
    pub classification: AnomalyClassification,
    pub signature_hash: String,
    pub prompt_sha256: Option<String>,
    pub block_text: Option<String>,
    pub unknown_blocks: Option<Value>,
    pub count: i64,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub observations: Vec<AnomalyObservation>,
    pub sub_pattern: crate::compose_prompt::sub_patterns::SubPattern,
    pub firing_rule_ids: Vec<String>,
    pub persona: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnomalyClassification {
    UnknownClass,
    StructuralMismatch,
}

impl AnomalyClassification {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UnknownClass => "unknown-class",
            Self::StructuralMismatch => "structural-mismatch",
        }
    }
}
