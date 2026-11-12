use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{RuntimeContractError, validate_text, validate_uuid};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservationSource {
    pub kind: String,
    pub version: Option<String>,
    pub fidelity: String,
    pub completeness: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionLifecycle {
    Active,
    Idle,
    Ended,
    Failed,
}

impl SessionLifecycle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Idle => "idle",
            Self::Ended => "ended",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionControl {
    Managed,
    HookBoundary,
    PullOnly,
}

impl SessionControl {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Managed => "managed",
            Self::HookBoundary => "hook_boundary",
            Self::PullOnly => "pull_only",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageSnapshot {
    pub scope: String,
    pub basis: String,
    pub input_tokens: Option<i64>,
    pub cached_input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub reasoning_output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub context_window: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCallObservation {
    pub call_id: String,
    pub status: Option<String>,
    pub duration_ms: Option<i64>,
    pub http_status: Option<i16>,
    pub usage: Option<UsageSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeItemObservation {
    pub item_id: String,
    pub turn_id: Option<String>,
    pub kind: String,
    pub name: Option<String>,
    pub status: Option<String>,
    pub success: Option<bool>,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionObservation {
    pub session_id: Uuid,
    pub observation_id: Uuid,
    pub runtime: String,
    pub source: ObservationSource,
    pub observed_at: DateTime<Utc>,
    pub sequence: i64,
    pub lifecycle: Option<SessionLifecycle>,
    pub requested_model: Option<String>,
    pub observed_model: Option<String>,
    pub usage: Option<UsageSnapshot>,
    pub model_call: Option<ModelCallObservation>,
    pub item: Option<RuntimeItemObservation>,
    pub control: Option<SessionControl>,
}

impl SessionObservation {
    pub fn validate(&self) -> Result<(), RuntimeContractError> {
        validate_uuid(self.session_id, "session_id")?;
        validate_uuid(self.observation_id, "observation_id")?;
        for (text, field) in [
            (&self.runtime, "runtime"),
            (&self.source.kind, "source_kind"),
            (&self.source.fidelity, "fidelity"),
            (&self.source.completeness, "completeness"),
        ] {
            validate_text(text, field)?;
        }
        if let Some(call) = &self.model_call {
            validate_text(&call.call_id, "call_id")?;
        }
        if let Some(item) = &self.item {
            validate_text(&item.item_id, "item_id")?;
            validate_text(&item.kind, "item_kind")?;
        }
        for usage in self
            .usage
            .iter()
            .chain(self.model_call.iter().filter_map(|c| c.usage.as_ref()))
        {
            validate_text(&usage.scope, "usage_scope")?;
            validate_text(&usage.basis, "usage_basis")?;
        }
        Ok(())
    }
}
