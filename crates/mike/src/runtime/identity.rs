use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{RuntimeContractError, validate_text, validate_uuid};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeKind {
    #[default]
    Claude,
    Codex,
}

impl RuntimeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }

    pub fn ensure_managed_launch_supported(self) -> Result<(), RuntimeContractError> {
        match self {
            Self::Claude => Ok(()),
            Self::Codex => Err(RuntimeContractError::UnsupportedManagedRuntime),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeNamespace {
    pub id: Uuid,
    pub runtime: RuntimeKind,
    pub durable_key: String,
}

impl RuntimeNamespace {
    pub fn validate(&self) -> Result<(), RuntimeContractError> {
        validate_uuid(self.id, "namespace_id")?;
        validate_text(&self.durable_key, "durable_key")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderThreadRef {
    pub namespace_id: Uuid,
    pub thread_id: String,
}

impl ProviderThreadRef {
    pub fn validate(&self) -> Result<(), RuntimeContractError> {
        validate_uuid(self.namespace_id, "namespace_id")?;
        validate_text(&self.thread_id, "provider_thread_id")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeThreadBinding {
    pub mike_session_id: Uuid,
    pub provider: ProviderThreadRef,
    pub provider_session_id: Option<String>,
    pub owning_seat_id: Option<Uuid>,
    pub native_parent: Option<ProviderThreadRef>,
    pub forked_from: Option<ProviderThreadRef>,
}

impl RuntimeThreadBinding {
    pub fn validate(&self) -> Result<(), RuntimeContractError> {
        validate_uuid(self.mike_session_id, "mike_session_id")?;
        self.provider.validate()?;
        if let Some(id) = &self.provider_session_id {
            validate_text(id, "provider_session_id")?;
        }
        if let Some(id) = self.owning_seat_id {
            validate_uuid(id, "owning_seat_id")?;
        }
        self.validate_ancestry(self.native_parent.as_ref(), "native parent")?;
        self.validate_ancestry(self.forked_from.as_ref(), "fork ancestor")?;
        if self.native_parent.is_some() && self.owning_seat_id.is_none() {
            return Err(RuntimeContractError::MissingOwningSeat);
        }
        if self.native_parent.is_some() && self.owning_seat_id == Some(self.mike_session_id) {
            return Err(RuntimeContractError::ChildOwnsItself);
        }
        Ok(())
    }

    fn validate_ancestry(
        &self,
        ancestor: Option<&ProviderThreadRef>,
        relation: &'static str,
    ) -> Result<(), RuntimeContractError> {
        if let Some(ancestor) = ancestor {
            ancestor.validate()?;
            if ancestor == &self.provider {
                return Err(RuntimeContractError::SelfReference(relation));
            }
        }
        Ok(())
    }
}
