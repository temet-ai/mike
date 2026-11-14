use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventKind {
    PlanVersioned,
    SeatLaunchRequested,
    SeatLaunchConfirmed,
    SeatLaunchFailed,
    SeatFirstRequest,
    SeatHeartbeatStale,
    SeatProcessGone,
    SeatClosed,
    SeatWoken,
    CommitLanded,
    TreeBrokenBy,
    CheckQueued,
    CheckFinished,
    FloorPassed,
    FloorFailed,
    ReportFiled,
    VerdictRecorded,
    CriterionSuspect,
    QuestionAsked,
    QuestionAnswered,
    ProblemOpened,
    ProblemDecided,
    ProblemEscalated,
    StopRequested,
    OperatorTouched,
    ResourceAdmitted,
}

impl EventKind {
    pub const ALL: [EventKind; 26] = [
        EventKind::PlanVersioned,
        EventKind::SeatLaunchRequested,
        EventKind::SeatLaunchConfirmed,
        EventKind::SeatLaunchFailed,
        EventKind::SeatFirstRequest,
        EventKind::SeatHeartbeatStale,
        EventKind::SeatProcessGone,
        EventKind::SeatClosed,
        EventKind::SeatWoken,
        EventKind::CommitLanded,
        EventKind::TreeBrokenBy,
        EventKind::CheckQueued,
        EventKind::CheckFinished,
        EventKind::FloorPassed,
        EventKind::FloorFailed,
        EventKind::ReportFiled,
        EventKind::VerdictRecorded,
        EventKind::CriterionSuspect,
        EventKind::QuestionAsked,
        EventKind::QuestionAnswered,
        EventKind::ProblemOpened,
        EventKind::ProblemDecided,
        EventKind::ProblemEscalated,
        EventKind::StopRequested,
        EventKind::OperatorTouched,
        EventKind::ResourceAdmitted,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            EventKind::PlanVersioned => "plan_versioned",
            EventKind::SeatLaunchRequested => "seat_launch_requested",
            EventKind::SeatLaunchConfirmed => "seat_launch_confirmed",
            EventKind::SeatLaunchFailed => "seat_launch_failed",
            EventKind::SeatFirstRequest => "seat_first_request",
            EventKind::SeatHeartbeatStale => "seat_heartbeat_stale",
            EventKind::SeatProcessGone => "seat_process_gone",
            EventKind::SeatClosed => "seat_closed",
            EventKind::SeatWoken => "seat_woken",
            EventKind::CommitLanded => "commit_landed",
            EventKind::TreeBrokenBy => "tree_broken_by",
            EventKind::CheckQueued => "check_queued",
            EventKind::CheckFinished => "check_finished",
            EventKind::FloorPassed => "floor_passed",
            EventKind::FloorFailed => "floor_failed",
            EventKind::ReportFiled => "report_filed",
            EventKind::VerdictRecorded => "verdict_recorded",
            EventKind::CriterionSuspect => "criterion_suspect",
            EventKind::QuestionAsked => "question_asked",
            EventKind::QuestionAnswered => "question_answered",
            EventKind::ProblemOpened => "problem_opened",
            EventKind::ProblemDecided => "problem_decided",
            EventKind::ProblemEscalated => "problem_escalated",
            EventKind::StopRequested => "stop_requested",
            EventKind::OperatorTouched => "operator_touched",
            EventKind::ResourceAdmitted => "resource_admitted",
        }
    }

    pub fn parse(name: &str) -> Option<EventKind> {
        EventKind::ALL.into_iter().find(|k| k.as_str() == name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanVersioned {
    pub version: i32,
    pub brief_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatLaunchRequested {
    pub launch_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatLaunchConfirmed {
    pub launch_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatLaunchFailed {
    pub launch_id: i64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatFirstRequest {
    pub request_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatHeartbeatStale {
    pub silent_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatProcessGone {
    pub launch_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatClosed {
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatWoken {
    pub via: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitLanded {
    pub sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeBrokenBy {
    pub sha: String,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckQueued {
    pub receipt_id: i64,
    pub operation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckFinished {
    pub receipt_id: i64,
    pub sha: String,
    pub passed: bool,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub exit: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FloorPassed {
    pub sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FloorFailed {
    pub sha: String,
    pub check: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportFiled {
    pub note_uuid: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerdictRecorded {
    pub note_uuid: Uuid,
    pub criterion: String,
    pub passed: bool,
    #[serde(default)]
    pub needed: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CriterionSuspect {
    pub criterion: String,
    pub failures: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionAsked {
    pub question_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionAnswered {
    pub question_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProblemOpened {
    pub problem_id: Uuid,
    pub trigger: String,
    #[serde(default)]
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProblemDecided {
    pub problem_id: Uuid,
    pub decided_by: String,
    #[serde(default)]
    pub decision: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub evidence_read: Vec<String>,
    #[serde(default)]
    pub minutes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProblemEscalated {
    pub problem_id: Uuid,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub question: String,
    #[serde(default)]
    pub routed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopRequested {
    pub by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorTouched {
    pub what: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceAdmitted {
    pub queued_at: DateTime<Utc>,
    pub admitted_at: DateTime<Utc>,
    pub resource: String,
    pub units: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum EventPayload {
    PlanVersioned(PlanVersioned),
    SeatLaunchRequested(SeatLaunchRequested),
    SeatLaunchConfirmed(SeatLaunchConfirmed),
    SeatLaunchFailed(SeatLaunchFailed),
    SeatFirstRequest(SeatFirstRequest),
    SeatHeartbeatStale(SeatHeartbeatStale),
    SeatProcessGone(SeatProcessGone),
    SeatClosed(SeatClosed),
    SeatWoken(SeatWoken),
    CommitLanded(CommitLanded),
    TreeBrokenBy(TreeBrokenBy),
    CheckQueued(CheckQueued),
    CheckFinished(CheckFinished),
    FloorPassed(FloorPassed),
    FloorFailed(FloorFailed),
    ReportFiled(ReportFiled),
    VerdictRecorded(VerdictRecorded),
    CriterionSuspect(CriterionSuspect),
    QuestionAsked(QuestionAsked),
    QuestionAnswered(QuestionAnswered),
    ProblemOpened(ProblemOpened),
    ProblemDecided(ProblemDecided),
    ProblemEscalated(ProblemEscalated),
    StopRequested(StopRequested),
    OperatorTouched(OperatorTouched),
    ResourceAdmitted(ResourceAdmitted),
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewFleetEvent {
    pub run_id: String,
    pub task: Option<String>,
    pub session_uuid: Option<Uuid>,
    pub payload: EventPayload,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FleetEvent {
    pub id: i64,
    pub run_id: String,
    pub task: Option<String>,
    pub session_uuid: Option<Uuid>,
    pub kind: EventKind,
    pub payload: serde_json::Value,
    pub occurred_at: DateTime<Utc>,
}

impl EventPayload {
    pub fn kind(&self) -> Result<EventKind, serde_json::Error> {
        let tagged = serde_json::to_value(self)?;
        let name = tagged
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        EventKind::parse(name).ok_or_else(|| {
            serde::ser::Error::custom(format!("payload names no known event kind: {name}"))
        })
    }

    pub fn body(&self) -> Result<serde_json::Value, serde_json::Error> {
        let tagged = serde_json::to_value(self)?;
        Ok(tagged
            .get("payload")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})))
    }
}

impl FleetEvent {
    pub fn decode(&self) -> Result<EventPayload, serde_json::Error> {
        serde_json::from_value(serde_json::json!({
            "kind": self.kind.as_str(),
            "payload": self.payload,
        }))
    }
}
