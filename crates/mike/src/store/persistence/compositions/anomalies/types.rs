use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

#[derive(sqlx::FromRow)]
pub(super) struct AnomalyRow {
    pub(super) classification: Option<String>,
    pub(super) prompt_sha256: Option<String>,
    pub(super) unknown_blocks: Option<Value>,
    pub(super) block_text: Option<String>,
    pub(super) occurrence_count: Option<i64>,
    pub(super) first_seen: Option<DateTime<Utc>>,
    pub(super) last_seen: Option<DateTime<Utc>>,
    pub(super) observations: sqlx::types::Json<Vec<AnomalyObservation>>,
    pub(super) group_firing_rule_ids: Option<Vec<String>>,
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
pub(crate) struct PromptClassScope<'a> {
    pub(crate) model: Option<&'a str>,
    pub(crate) project_path: Option<&'a str>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct PromptClassRow {
    pub(crate) lane: String,
    pub(crate) subagent_persona: Option<String>,
    pub(crate) observation_count: i64,
    pub(crate) first_seen: Option<DateTime<Utc>>,
    pub(crate) last_seen: Option<DateTime<Utc>>,
    pub(crate) block_count: i32,
    pub(crate) model_breakdown: sqlx::types::Json<Vec<PromptClassModelCount>>,
    pub(crate) composition_id: Option<i64>,
    pub(crate) label: Option<String>,
    pub(crate) mode: Option<String>,
    pub(crate) agent_definition_path: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct PromptClassDetailRow {
    pub(crate) lane: String,
    pub(crate) subagent_persona: Option<String>,
    pub(crate) observation_count: i64,
    pub(crate) first_seen: Option<DateTime<Utc>>,
    pub(crate) last_seen: Option<DateTime<Utc>>,
    pub(crate) sample_blocks: Option<Value>,
    pub(crate) model_breakdown: sqlx::types::Json<Vec<PromptClassModelCount>>,
    pub(crate) composition_id: Option<i64>,
    pub(crate) label: Option<String>,
    pub(crate) mode: Option<String>,
    pub(crate) agent_definition_path: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct PromptClassObservationRow {
    pub(crate) request_id: i64,
    pub(crate) session_id: i64,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) content_sha256: Option<String>,
    pub(crate) altered: bool,
    pub(crate) model: Option<String>,
    pub(crate) total_count: i64,
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
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::UnknownClass => "unknown-class",
            Self::StructuralMismatch => "structural-mismatch",
        }
    }
}
