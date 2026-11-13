pub mod arm_rows;
pub mod arms;
pub mod events;
pub mod gate;
pub mod interventions_feed;
pub mod pending_wakes;
pub mod policies;
pub mod promotions;
pub mod verdict_label;

use chrono::{DateTime, Utc};
use serde::Serialize;

pub use arm_rows::ArmRow;
pub use arms::{AnchorFire, ArmWrite};
pub use events::{EventsFilter, QuestionEventRow};
pub use gate::{GateCounts, GateWindow, VerifierPairs};
pub use interventions_feed::InterventionFeedRow;
pub use pending_wakes::{DueWakeRow, ReachDueRow, WakeFinal};
pub use promotions::PromotionRow;

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct PolicyRow {
    pub id: i64,
    pub steerer_id: i64,
    pub question: String,
    pub mode: String,
    pub holdback_pct: Option<i16>,
    pub holdback_salt: uuid::Uuid,
    pub updated_at: DateTime<Utc>,
    pub updated_via: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModeChange<'a> {
    pub policy_id: i64,
    pub steerer_id: i64,
    pub question: &'a str,
    pub from_mode: &'a str,
    pub to_mode: &'a str,
    pub action: &'a str,
    pub decided_via: &'a str,
    pub gate_passed: bool,
    pub gate_json: &'a serde_json::Value,
    pub override_reason: Option<&'a str>,
    pub replaces_steerer_id: Option<i64>,
    pub restored_steerer_id: Option<i64>,
    pub source_json: &'a str,
}
