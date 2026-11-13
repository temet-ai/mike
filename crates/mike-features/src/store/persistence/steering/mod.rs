use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;

pub mod backfill;
#[cfg(test)]
pub mod efficiency_steerer_class_tests;
#[cfg(test)]
pub mod enrol_inert_steerers_tests;
pub mod enrolment;
pub mod fleet;
pub mod guard;
pub mod intervention;
pub mod promotion;
pub mod recent_delivery;
pub mod reply_labels;
#[cfg(test)]
pub mod update_merge_tests;
pub mod views;

pub use backfill::{
    backfill_delivered_request_id, backfill_sent_request_id, backfill_trigger_request_id,
};
pub use recent_delivery::{LastFire, SessionNote};
pub use reply_labels::SteererLabelWindow;
pub use reply_labels::{RegradeOlderVersions, ReplyLabelCandidate};

#[derive(sqlx::FromRow, Debug)]
pub struct SteererRow {
    pub id: i64,
    pub uuid: uuid::Uuid,
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub agent_id: i64,
    pub authority: String,
    pub mode: String,
    pub model: String,
    pub priority: i32,
    pub enabled: bool,
    pub config: JsonValue,
    pub prompt: Option<String>,
    pub kind: String,
    pub trigger_point: String,
    pub invocation: String,
    pub pattern: Option<String>,
    pub auto_enrol: bool,
    pub project_id: Option<i64>,
    pub project_path: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow, Debug)]
pub struct EnrolmentRow {
    pub id: i64,
    pub session_id: i64,
    pub steerer_id: i64,
    pub mode: String,
    pub authority: String,
    pub enrolled_at: DateTime<Utc>,
    pub enrolled_by: String,
    pub disenrolled_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
pub struct SteeredRequestRow {
    pub request_id: Option<i64>,
    pub response_id: Option<i64>,
    pub session_uuid: Option<uuid::Uuid>,
    pub operator_turn_id: Option<i64>,
    pub steerer_name: Option<String>,
    pub steerer_display_name: Option<String>,
    pub authority: Option<String>,
    pub mode: Option<String>,
    pub verdict: Option<String>,
    pub off_thread_topic: Option<String>,
    pub confidence: Option<f64>,
    pub payload_text: Option<String>,
    pub anchor_text: Option<String>,
    pub verdict_json: Option<JsonValue>,
    pub model: Option<String>,
    pub sent_at: Option<DateTime<Utc>>,
    pub processed_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub reverted_at: Option<DateTime<Utc>>,
    pub revert_reason: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub trigger_request_id: Option<i64>,
}

#[derive(sqlx::FromRow)]
pub struct SessionSteeringRow {
    pub intervention_id: Option<i64>,
    pub session_id: Option<i64>,
    pub session_uuid: Option<uuid::Uuid>,
    pub project_path: Option<String>,
    pub steerer_name: Option<String>,
    pub steerer_display_name: Option<String>,
    pub verdict: Option<String>,
    pub authority: Option<String>,
    pub mode: Option<String>,
    pub trigger_request_id: Option<i64>,
    pub sent_request_id: Option<i64>,
    pub sent_response_id: Option<i64>,
    pub operator_turn_id: Option<i64>,
    pub gate_reason: Option<String>,
    pub off_thread_topic: Option<String>,
    pub confidence: Option<f64>,
    pub anchor_text: Option<String>,
    pub verdict_json: Option<JsonValue>,
    pub payload_text: Option<String>,
    pub model: Option<String>,
    pub sent_at: Option<DateTime<Utc>>,
    pub processed_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub reverted_at: Option<DateTime<Utc>>,
    pub revert_reason: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
pub struct SessionEnrolmentRow {
    pub enrolment_id: Option<i64>,
    pub session_id: Option<i64>,
    pub session_uuid: Option<uuid::Uuid>,
    pub steerer_id: Option<i64>,
    pub steerer_name: Option<String>,
    pub steerer_display_name: Option<String>,
    pub steerer_enabled: Option<bool>,
    pub mode: Option<String>,
    pub authority: Option<String>,
    pub enrolled_at: Option<DateTime<Utc>>,
    pub enrolled_by: Option<String>,
    pub disenrolled_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow, Debug)]
pub struct VerifierRateRow {
    pub project_path: String,
    pub fires_count: i64,
    pub corrected_count: i64,
    pub delivered_count: i64,
    pub excluded_judge_failed_count: i64,
    pub corrected_rate: Option<f64>,
    pub delivered_rate: Option<f64>,
}

pub struct ArmingHistoryRow {
    pub closed_turn_count: i64,
    pub recent_outcomes_newest_first: Vec<String>,
}

#[derive(sqlx::FromRow, Debug)]
pub struct PriorFireRow {
    pub off_thread_topic: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub struct InterventionStaleness {
    pub expires_at: Option<DateTime<Utc>>,
    pub superseded: bool,
    pub pushed_at: Option<DateTime<Utc>>,
    pub acked_at: Option<DateTime<Utc>>,
}

pub struct SteererFields<'a> {
    pub name: &'a str,
    pub display_name: &'a str,
    pub description: &'a str,
    pub authority_code: &'a str,
    pub mode_code: &'a str,
    pub model: &'a str,
    pub priority: i32,
    pub enabled: bool,
    pub config: &'a JsonValue,
    pub prompt: Option<&'a str>,
    pub trigger_point: &'a str,
    pub invocation: &'a str,
    pub pattern: Option<&'a str>,
    pub project_id: Option<i64>,
}

pub struct SteererUpdate<'a> {
    pub id: i64,
    pub display_name: &'a str,
    pub description: &'a str,
    pub enabled: bool,
    pub authority_code: &'a str,
    pub mode_code: &'a str,
    pub model: &'a str,
    pub priority: i32,
    pub config: &'a JsonValue,
    pub prompt: Option<&'a str>,
    pub trigger_point: &'a str,
    pub invocation: &'a str,
    pub pattern: Option<&'a str>,
    pub project_id: Option<i64>,
}

pub struct NewIntervention<'a> {
    pub steerer_id: i64,
    pub enrolment_id: Option<i64>,
    pub session_id: i64,
    pub verdict_code: &'a str,
    pub mode_code: &'a str,
    pub authority_code: &'a str,
    pub trigger_request_id: Option<i64>,
    pub confidence: Option<f64>,
    pub off_thread_topic: Option<&'a str>,
    pub payload_text: Option<&'a str>,
    pub missing_evidence: Option<&'a str>,
    pub model: Option<&'a str>,
    pub anchor_text: Option<&'a str>,
    pub intervention_uuid: uuid::Uuid,
    pub expires_at: DateTime<Utc>,
    pub verdict_json: Option<JsonValue>,
    pub operator_turn_id: Option<i64>,
    pub bundle_id: Option<i64>,
    pub delivery_channel: Option<&'a str>,
    pub addressed_to: Option<&'a str>,
    pub deliver_after: Option<DateTime<Utc>>,
    pub wake_state: Option<&'a str>,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct VocabTermRow {
    pub code: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct SteeringVocabulary {
    pub authorities: Vec<VocabTermRow>,
    pub modes: Vec<VocabTermRow>,
    pub verdicts: Vec<VocabTermRow>,
}
