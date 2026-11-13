pub mod dispatch;
pub mod labels;
pub mod reads;
pub mod resolve;
#[cfg(test)]
mod tests;

use chrono::{DateTime, Utc};
use serde::Serialize;

pub use reads::{EvaluationFilter, EvaluationStatsFilter};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EvaluationRow {
    pub uuid: uuid::Uuid,
    pub operator_turn_id: i64,
    pub status: String,
    pub intervention_uuid: Option<uuid::Uuid>,
    pub dispatched_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

pub const UNRECORDED_ABSTENTION: &str = "no_judgement_recorded";

#[derive(Debug, Clone)]
pub struct EvaluationVerdict {
    pub verdict_json: serde_json::Value,
    pub confidence: Option<f64>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvaluationMode {
    Live,
    Shadow,
}

impl EvaluationMode {
    pub fn code(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Shadow => "shadow",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewEvaluation<'a> {
    pub steerer_id: i64,
    pub session_id: i64,
    pub operator_turn_id: i64,
    pub mode: EvaluationMode,
    pub bundle_id: Option<i64>,
    pub trigger_point: Option<&'a str>,
    pub trigger_request_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EvaluationLabel {
    pub uuid: uuid::Uuid,
    pub label: String,
    pub source: String,
    pub evidence: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct EvaluationListRow {
    pub id: i64,
    pub uuid: uuid::Uuid,
    pub steerer_id: i64,
    pub steerer_name: String,
    pub steerer_kind: String,
    pub operator_turn_id: i64,
    pub status: String,
    pub mode: String,
    pub question: Option<String>,
    pub trigger_point: Option<String>,
    pub trigger_request_id: Option<i64>,
    pub bundle_id: Option<i64>,
    pub intervention_uuid: Option<uuid::Uuid>,
    pub dispatched_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub verdict_json: Option<serde_json::Value>,
    pub confidence: Option<f64>,
    pub reason: Option<String>,
    pub outcome_label: Option<String>,
    pub label_source: Option<String>,
    pub labelled_at: Option<DateTime<Utc>>,
    pub label_evidence: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct EvaluationStatsRow {
    pub steerer_id: i64,
    pub steerer_name: String,
    pub question: Option<String>,
    pub mode: String,
    pub evaluations: i64,
    pub in_flight: i64,
    pub fires: i64,
    pub passes: i64,
    pub tp: i64,
    pub fp: i64,
    pub tn: i64,
    #[sqlx(rename = "fn")]
    #[serde(rename = "fn")]
    pub false_negatives: i64,
    pub indeterminate: i64,
    pub unlabelled: i64,
    pub non_actor_fires: i64,
}
