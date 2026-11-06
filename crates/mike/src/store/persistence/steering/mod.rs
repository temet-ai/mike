use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;

mod backfill;
#[cfg(test)]
mod efficiency_steerer_class_tests;
#[cfg(test)]
mod enrol_inert_steerers_tests;
mod enrolment;
mod fleet;
pub mod guard;
mod intervention;
pub mod promotion;
mod recent_delivery;
mod reply_labels;
#[cfg(test)]
mod update_merge_tests;
mod views;

pub(crate) use backfill::{
    backfill_delivered_request_id, backfill_sent_request_id, backfill_trigger_request_id,
};
pub use recent_delivery::{LastFire, SessionNote};
pub(crate) use reply_labels::SteererLabelWindow;
pub use reply_labels::{RegradeOlderVersions, ReplyLabelCandidate};

#[derive(sqlx::FromRow, Debug)]
pub(crate) struct SteererRow {
    pub(crate) id: i64,
    pub(crate) uuid: uuid::Uuid,
    pub(crate) name: String,
    pub(crate) display_name: String,
    pub(crate) description: String,
    pub(crate) agent_id: i64,
    pub(crate) authority: String,
    pub(crate) mode: String,
    pub(crate) model: String,
    pub(crate) priority: i32,
    pub(crate) enabled: bool,
    pub(crate) config: JsonValue,
    pub(crate) prompt: Option<String>,
    pub(crate) kind: String,
    pub(crate) trigger_point: String,
    pub(crate) invocation: String,
    pub(crate) pattern: Option<String>,
    pub(crate) auto_enrol: bool,
    pub(crate) project_id: Option<i64>,
    pub(crate) project_path: Option<String>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow, Debug)]
pub(crate) struct EnrolmentRow {
    pub(crate) id: i64,
    pub(crate) session_id: i64,
    pub(crate) steerer_id: i64,
    pub(crate) mode: String,
    pub(crate) authority: String,
    pub(crate) enrolled_at: DateTime<Utc>,
    pub(crate) enrolled_by: String,
    pub(crate) disenrolled_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
pub(crate) struct SteeredRequestRow {
    pub(crate) request_id: Option<i64>,
    pub(crate) response_id: Option<i64>,
    pub(crate) session_uuid: Option<uuid::Uuid>,
    pub(crate) operator_turn_id: Option<i64>,
    pub(crate) steerer_name: Option<String>,
    pub(crate) steerer_display_name: Option<String>,
    pub(crate) authority: Option<String>,
    pub(crate) mode: Option<String>,
    pub(crate) verdict: Option<String>,
    pub(crate) off_thread_topic: Option<String>,
    pub(crate) confidence: Option<f64>,
    pub(crate) payload_text: Option<String>,
    pub(crate) anchor_text: Option<String>,
    pub(crate) verdict_json: Option<JsonValue>,
    pub(crate) model: Option<String>,
    pub(crate) sent_at: Option<DateTime<Utc>>,
    pub(crate) processed_at: Option<DateTime<Utc>>,
    pub(crate) expires_at: Option<DateTime<Utc>>,
    pub(crate) reverted_at: Option<DateTime<Utc>>,
    pub(crate) revert_reason: Option<String>,
    pub(crate) created_at: Option<DateTime<Utc>>,
    pub(crate) trigger_request_id: Option<i64>,
}

#[derive(sqlx::FromRow)]
pub(crate) struct SessionSteeringRow {
    pub(crate) intervention_id: Option<i64>,
    pub(crate) session_id: Option<i64>,
    pub(crate) session_uuid: Option<uuid::Uuid>,
    pub(crate) project_path: Option<String>,
    pub(crate) steerer_name: Option<String>,
    pub(crate) steerer_display_name: Option<String>,
    pub(crate) verdict: Option<String>,
    pub(crate) authority: Option<String>,
    pub(crate) mode: Option<String>,
    pub(crate) trigger_request_id: Option<i64>,
    pub(crate) sent_request_id: Option<i64>,
    pub(crate) sent_response_id: Option<i64>,
    pub(crate) operator_turn_id: Option<i64>,
    pub(crate) gate_reason: Option<String>,
    pub(crate) off_thread_topic: Option<String>,
    pub(crate) confidence: Option<f64>,
    pub(crate) anchor_text: Option<String>,
    pub(crate) verdict_json: Option<JsonValue>,
    pub(crate) payload_text: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) sent_at: Option<DateTime<Utc>>,
    pub(crate) processed_at: Option<DateTime<Utc>>,
    pub(crate) expires_at: Option<DateTime<Utc>>,
    pub(crate) reverted_at: Option<DateTime<Utc>>,
    pub(crate) revert_reason: Option<String>,
    pub(crate) created_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
pub(crate) struct SessionEnrolmentRow {
    pub(crate) enrolment_id: Option<i64>,
    pub(crate) session_id: Option<i64>,
    pub(crate) session_uuid: Option<uuid::Uuid>,
    pub(crate) steerer_id: Option<i64>,
    pub(crate) steerer_name: Option<String>,
    pub(crate) steerer_display_name: Option<String>,
    pub(crate) steerer_enabled: Option<bool>,
    pub(crate) mode: Option<String>,
    pub(crate) authority: Option<String>,
    pub(crate) enrolled_at: Option<DateTime<Utc>>,
    pub(crate) enrolled_by: Option<String>,
    pub(crate) disenrolled_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow, Debug)]
pub(crate) struct VerifierRateRow {
    pub(crate) project_path: String,
    pub(crate) fires_count: i64,
    pub(crate) corrected_count: i64,
    pub(crate) delivered_count: i64,
    pub(crate) excluded_judge_failed_count: i64,
    pub(crate) corrected_rate: Option<f64>,
    pub(crate) delivered_rate: Option<f64>,
}

pub(crate) struct ArmingHistoryRow {
    pub(crate) closed_turn_count: i64,
    pub(crate) recent_outcomes_newest_first: Vec<String>,
}

#[derive(sqlx::FromRow, Debug)]
pub(crate) struct PriorFireRow {
    pub(crate) off_thread_topic: Option<String>,
    pub(crate) created_at: DateTime<Utc>,
}

pub(crate) struct InterventionStaleness {
    pub(crate) expires_at: Option<DateTime<Utc>>,
    pub(crate) superseded: bool,
    pub(crate) pushed_at: Option<DateTime<Utc>>,
    pub(crate) acked_at: Option<DateTime<Utc>>,
}

pub(crate) struct SteererFields<'a> {
    pub(crate) name: &'a str,
    pub(crate) display_name: &'a str,
    pub(crate) description: &'a str,
    pub(crate) authority_code: &'a str,
    pub(crate) mode_code: &'a str,
    pub(crate) model: &'a str,
    pub(crate) priority: i32,
    pub(crate) enabled: bool,
    pub(crate) config: &'a JsonValue,
    pub(crate) prompt: Option<&'a str>,
    pub(crate) trigger_point: &'a str,
    pub(crate) invocation: &'a str,
    pub(crate) pattern: Option<&'a str>,
    pub(crate) project_id: Option<i64>,
}

pub(crate) struct SteererUpdate<'a> {
    pub(crate) id: i64,
    pub(crate) display_name: &'a str,
    pub(crate) description: &'a str,
    pub(crate) enabled: bool,
    pub(crate) authority_code: &'a str,
    pub(crate) mode_code: &'a str,
    pub(crate) model: &'a str,
    pub(crate) priority: i32,
    pub(crate) config: &'a JsonValue,
    pub(crate) prompt: Option<&'a str>,
    pub(crate) trigger_point: &'a str,
    pub(crate) invocation: &'a str,
    pub(crate) pattern: Option<&'a str>,
    pub(crate) project_id: Option<i64>,
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
pub(crate) struct VocabTermRow {
    pub(crate) code: String,
    pub(crate) description: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SteeringVocabulary {
    pub(crate) authorities: Vec<VocabTermRow>,
    pub(crate) modes: Vec<VocabTermRow>,
    pub(crate) verdicts: Vec<VocabTermRow>,
}
