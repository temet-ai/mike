pub mod gates;
pub mod items;
pub mod labels_read;
pub mod review;
pub mod runs;
pub mod turn_texts;

use chrono::{DateTime, Utc};
use serde::Serialize;

pub use items::RunItemsPage;
pub use labels_read::LabelSetFilter;
pub use review::{EvalReviewContext, EvalReviewLabels};

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct LabelSetItem {
    pub operator_turn_id: i64,
    pub reaction_label_id: i64,
    pub reaction_label: String,
    pub delivery_label_id: Option<i64>,
    pub delivery_label: Option<String>,
    pub inclusion_prob: Option<f64>,
    pub stratum: String,
    pub control: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct EvalRunRow {
    pub id: i64,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub purpose: String,
    pub status: String,
    pub bundle_id: i64,
    pub rubric_version: String,
    pub input_builder_version: String,
    pub slices: Vec<String>,
    pub model: String,
    pub split: String,
    pub tiers: Vec<String>,
    pub holdout_batch: Option<String>,
    pub labels_as_of: DateTime<Utc>,
    pub label_count: i32,
    pub label_ids_hash: String,
    pub repeat_of_run_id: Option<i64>,
    pub max_usd: f64,
    pub cost_usd: f64,
    pub items_judged: i32,
    pub items_refused: i32,
    pub items_failed: i32,
    pub report: Option<serde_json::Value>,
    pub argv: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewEvalRun<'a> {
    pub purpose: &'a str,
    pub bundle_id: i64,
    pub rubric_version: &'a str,
    pub input_builder_version: &'a str,
    pub model: &'a str,
    pub split: &'a str,
    pub tiers: &'a [String],
    pub holdout_batch: Option<&'a str>,
    pub labels_as_of: DateTime<Utc>,
    pub label_count: i32,
    pub label_ids_hash: &'a str,
    pub repeat_of_run_id: Option<i64>,
    pub max_usd: f64,
    pub argv: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct EvalRunItemRow {
    pub run_id: i64,
    pub operator_turn_id: i64,
    pub reaction_label_id: Option<i64>,
    pub delivery_label_id: Option<i64>,
    pub turn_verdict_id: Option<i64>,
    pub outcome: String,
    pub stratum: String,
    pub inclusion_prob: Option<f64>,
    pub reaction_correct: Option<bool>,
    pub delivery_correct: Option<bool>,
    pub created_at: DateTime<Utc>,
    pub gold_reaction: Option<String>,
    pub gold_delivery: Option<String>,
    pub gold_control: Option<bool>,
    pub verdict_reaction: Option<String>,
    pub verdict_delivery: Option<String>,
    pub verdict_legacy_label: Option<String>,
    pub next_state: Option<String>,
    pub verdict: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gold_severity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_reaction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_delivery: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_uuid: Option<uuid::Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_title: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunItemWrite<'a> {
    pub run_id: i64,
    pub operator_turn_id: i64,
    pub reaction_label_id: Option<i64>,
    pub delivery_label_id: Option<i64>,
    pub turn_verdict_id: Option<i64>,
    pub outcome: &'a str,
    pub stratum: &'a str,
    pub inclusion_prob: Option<f64>,
    pub reaction_correct: Option<bool>,
    pub delivery_correct: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunItemsFilter {
    pub outcome: Option<String>,
    pub reaction_correct: Option<bool>,
    pub delivery_correct: Option<bool>,
    pub disagreement: Option<Disagreement>,
    pub labels: ItemLabelFilter,
    pub reviewed: Option<bool>,
    pub operator_turn_id: Option<i64>,
    pub include_verdict: bool,
    pub include_review: bool,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disagreement {
    Any,
    Reaction,
    Delivery,
}

impl Disagreement {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "any" => Some(Self::Any),
            "reaction" => Some(Self::Reaction),
            "delivery" => Some(Self::Delivery),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemLabelFilter {
    pub gold_reaction: Option<String>,
    pub gold_delivery: Option<String>,
    pub verdict_reaction: Option<String>,
    pub verdict_delivery: Option<String>,
    pub severity: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunsFilter {
    pub status: Option<String>,
    pub role: Option<String>,
    pub split: Option<String>,
    pub purpose: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct EvalGateRow {
    pub id: i64,
    pub created_at: DateTime<Utc>,
    pub candidate_run_id: i64,
    pub baseline_run_id: Option<i64>,
    pub noise_run_id: Option<i64>,
    pub holdout_batch: Option<String>,
    pub verdict: String,
    pub floors: serde_json::Value,
    pub report: serde_json::Value,
    pub override_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewEvalGate<'a> {
    pub candidate_run_id: i64,
    pub baseline_run_id: Option<i64>,
    pub noise_run_id: Option<i64>,
    pub holdout_batch: Option<&'a str>,
    pub verdict: &'a str,
    pub floors: serde_json::Value,
    pub report: serde_json::Value,
    pub override_reason: Option<&'a str>,
}
