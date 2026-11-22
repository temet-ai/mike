use chrono::{DateTime, Utc};

use crate::workspace::Liveness;

pub mod flow_admission;
pub use flow_admission::{FlowAdmissionEvidence, FlowAdmissionResult};
pub use launch_brief::QueuedLaunchBrief;

pub mod launch_brief;
pub mod ledger;
pub mod project_lookup;
pub use project_lookup::PathMatch;
pub mod reap;
pub mod registration;
pub mod roster;
pub mod side_call_roster;

pub const HEARTBEAT_LEASE_MINUTES: i64 = 15;

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

const KNOWN_LEASES_CAPACITY: usize = 20_000;

/// Lease expiries this process wrote or read, by session row id. A session
/// heartbeat renews its lease about once a minute; a Remote Control
/// heartbeat reads this to see that the lease needs nothing from it.
#[derive(Default)]
struct KnownLeases(dashmap::DashMap<i64, DateTime<Utc>>);

impl super::PersistHandle {
    pub fn note_lease(&self, session_id: i64, expires_at: DateTime<Utc>) {
        let known = self.memo::<KnownLeases>();
        if known.0.len() >= KNOWN_LEASES_CAPACITY {
            known.0.clear();
        }
        known.0.insert(session_id, expires_at);
    }

    pub fn known_lease(&self, session_id: i64) -> Option<DateTime<Utc>> {
        self.memo::<KnownLeases>().0.get(&session_id).map(|at| *at)
    }
}

/// Set when this process releases a workspace registration (session end, reap,
/// runtime end). Launch gone-closure reads it to look again at once: a release
/// is what lets a launch whose session was registered close.
#[derive(Default)]
struct RegistrationsReleased(std::sync::atomic::AtomicBool);

impl super::PersistHandle {
    pub fn note_registration_released(&self) {
        self.memo::<RegistrationsReleased>()
            .0
            .store(true, std::sync::atomic::Ordering::Release);
    }

    /// Clears the mark and says whether a release happened since the last call.
    pub fn take_registration_released(&self) -> bool {
        self.memo::<RegistrationsReleased>()
            .0
            .swap(false, std::sync::atomic::Ordering::AcqRel)
    }
}

#[derive(Debug, Clone)]
pub struct CoordinationEventRow {
    pub id: i64,
    pub session_id: Option<uuid::Uuid>,
    pub actor_label: String,
    pub kind: String,
    pub subject: String,
    pub detail: serde_json::Value,
    pub created_at: DateTime<Utc>,
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
    pub machine: Option<super::session_machine::MachineRef>,
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
pub struct ReapCandidate {
    pub session_id: i64,
    /// Set when the registration was started on this boot and names a process.
    pub pid: Option<i32>,
    /// The lease lapsed more than the grace period ago.
    pub lapsed: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ReapOutcome {
    pub reaped: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndRegistrationOutcome {
    Ended,
    Rejected,
}

#[derive(Debug, Clone)]
pub enum ProjectPathResolution {
    Resolved(i64, String),
    Ambiguous(Vec<String>),
    NotFound,
}
