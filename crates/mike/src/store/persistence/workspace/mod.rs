use chrono::{DateTime, Utc};

use crate::workspace::Liveness;

pub mod flow_admission;
pub use flow_admission::{FlowAdmissionEvidence, FlowAdmissionResult};
pub use launch_brief::QueuedLaunchBrief;

mod launch_brief;
mod ledger;
mod project_lookup;
pub(crate) use project_lookup::PathMatch;
mod reap;
mod registration;
mod roster;

pub(crate) const HEARTBEAT_LEASE_MINUTES: i64 = 15;

pub struct RegisterWorkspaceSession<'a> {
    pub session_uuid: uuid::Uuid,
    pub project_path: Option<&'a str>,
    pub cwd: &'a str,
    pub worktree: Option<&'a str>,
    pub registered_via: &'a str,
    pub intent: Option<(&'a str, &'a str)>,
    pub role: Option<&'a str>,
    pub name: Option<&'a str>,
    pub instructions: Option<&'a str>,
    pub pid: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct RegisteredWorkspaceSession {
    pub flow_admission: FlowAdmissionResult,
    pub project_id: i64,
    pub project_path: Option<String>,
    pub project_resolution: String,
    pub registered_via: &'static str,
    pub lease_expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy)]
pub struct RenewedHeartbeat {
    pub session_id: i64,
    pub lease_expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub(crate) struct CoordinationEventRow {
    pub(crate) id: i64,
    pub(crate) session_id: Option<uuid::Uuid>,
    pub(crate) actor_label: String,
    pub(crate) kind: String,
    pub(crate) subject: String,
    pub(crate) detail: serde_json::Value,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceRosterRow {
    pub session_uuid: uuid::Uuid,
    pub cwd: String,
    pub worktree: Option<String>,
    pub registered_via: String,
    pub registered_at: DateTime<Utc>,
    pub lease_expires_at: DateTime<Utc>,
    pub released_at: Option<DateTime<Utc>>,
    pub goal: Option<String>,
    pub next_steps: Option<String>,
    pub display_name: Option<String>,
    pub role: Option<String>,
    pub instructions: Option<String>,
    pub liveness: Liveness,
    pub last_request_at: Option<DateTime<Utc>>,
    pub newest_request_number: Option<i64>,
    pub channel_present: bool,
    pub channel_cse_id: Option<String>,
    pub heartbeat_age_seconds: Option<i64>,
    pub heartbeat_interval_seconds: Option<i32>,
    pub idle_seconds: Option<i32>,
    pub session_state: Option<String>,
    pub session_state_at: Option<DateTime<Utc>>,
    pub reply_channel_open: bool,
    pub reply_channel_opened_at: Option<DateTime<Utc>>,
    pub last_inbound_message_at: Option<DateTime<Utc>>,
    pub post_turn_summary: Option<serde_json::Value>,
    pub post_turn_summary_at: Option<DateTime<Utc>>,
    pub client_version: Option<String>,
    pub liveness_source: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ReapCandidate {
    pub(crate) session_id: i64,
    pub(crate) pid: i32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ReapOutcome {
    pub(crate) reaped: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndRegistrationOutcome {
    Ended,
    Rejected,
}

#[derive(Debug, Clone)]
pub(crate) enum ProjectPathResolution {
    Resolved(i64, String),
    Ambiguous(Vec<String>),
    NotFound,
}
