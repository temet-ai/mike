use uuid::Uuid;

use crate::runtime::codex::{
    ExclusionReason, ProjectedItem, ProjectionEffect, ProjectionIssue, ProjectionStatus,
};
use crate::runtime::{RuntimeEventEnvelope, RuntimeObservation};

use super::super::RuntimeWriteOutcome;
use super::CodexIntakeContext;

#[derive(Debug, Clone)]
pub struct CodexReceiptInput {
    pub envelope: RuntimeEventEnvelope,
    pub raw_payload: Vec<u8>,
    pub context: CodexIntakeContext,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRuntimeReceipt {
    pub event_id: i64,
    pub observation: RuntimeObservation,
    pub capture_sequence: Option<i64>,
    pub context: Option<CodexIntakeContext>,
    pub transport_gap: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexCaptureOutcome {
    Captured {
        receipt: Box<StoredRuntimeReceipt>,
        outcome: RuntimeWriteOutcome,
    },
    Excluded(ExclusionReason),
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct CodexGeneration {
    pub generation_id: Uuid,
    pub namespace_id: Uuid,
    pub projector_version: i32,
    pub state_codec_version: i32,
    pub state: String,
    pub last_applied_capture_sequence: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexProjectionApplication {
    pub generation_id: Uuid,
    pub event_id: i64,
    pub capture_sequence: i64,
    pub status: ProjectionStatus,
    pub issues: Vec<ProjectionIssue>,
    pub effect_count: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexProjectionApplyOutcome {
    Applied(CodexProjectionApplication),
    CaughtUp(CodexGeneration),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexProjectionEffect {
    pub event_id: i64,
    pub capture_sequence: i64,
    pub effect_ordinal: i32,
    pub effect: ProjectionEffect,
}

#[derive(Debug, Clone)]
pub struct RawEventPage {
    pub namespace_id: Uuid,
    pub after_event_id: i64,
    pub limit: i64,
}

#[derive(Debug, Clone)]
pub struct CodexEffectPage {
    pub generation_id: Uuid,
    pub thread_id: String,
    pub kind: Option<String>,
    pub after: Option<(i64, i32)>,
    pub limit: i64,
}

#[derive(Debug, Clone)]
pub struct CodexItemPage {
    pub generation_id: Uuid,
    pub thread_id: String,
    pub after: Option<(i64, String, String)>,
    pub limit: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexStoredItem {
    pub first_capture_sequence: i64,
    pub latest_capture_sequence: i64,
    pub item: ProjectedItem,
}

pub(super) fn status_code(status: ProjectionStatus) -> &'static str {
    match status {
        ProjectionStatus::Projected => "projected",
        ProjectionStatus::Partial => "partial",
        ProjectionStatus::Unsupported => "unsupported",
        ProjectionStatus::Malformed => "malformed",
        ProjectionStatus::Excluded => "excluded",
    }
}

pub(super) fn parse_status(status: &str) -> Result<ProjectionStatus, super::PersistError> {
    Ok(match status {
        "projected" => ProjectionStatus::Projected,
        "partial" => ProjectionStatus::Partial,
        "unsupported" => ProjectionStatus::Unsupported,
        "malformed" => ProjectionStatus::Malformed,
        "excluded" => ProjectionStatus::Excluded,
        _ => return Err(super::invariant("unknown projection application status")),
    })
}
