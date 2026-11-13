use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

    pub fn controls_through_terminal(self) -> bool {
        match self {
            Self::Claude => true,
            Self::Codex => false,
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

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RuntimeContractError {
    #[error("runtime field {0} must be nonempty and contain no NUL characters")]
    InvalidText(&'static str),
    #[error("runtime field {0} must not be a nil UUID")]
    InvalidUuid(&'static str),
    #[error("a runtime thread cannot be its own {0}")]
    SelfReference(&'static str),
    #[error("native child threads require an owning Mike seat")]
    MissingOwningSeat,
    #[error("a native child must have a Mike identity distinct from its owning seat")]
    ChildOwnsItself,
    #[error("runtime envelope version must be positive")]
    InvalidEnvelopeVersion,
    #[error("runtime extension field {0} conflicts with a defined envelope field")]
    ConflictingExtension(String),
    #[error("parsed runtime payload does not match the original bytes")]
    InconsistentProjection,
}

pub fn validate_text(value: &str, field: &'static str) -> Result<(), RuntimeContractError> {
    if value.trim().is_empty() || value.contains('\0') {
        return Err(RuntimeContractError::InvalidText(field));
    }
    Ok(())
}

pub fn validate_uuid(value: uuid::Uuid, field: &'static str) -> Result<(), RuntimeContractError> {
    if value.is_nil() {
        return Err(RuntimeContractError::InvalidUuid(field));
    }
    Ok(())
}

pub fn validate_extensions(
    extra: &std::collections::BTreeMap<String, serde_json::Value>,
    reserved: &[&str],
) -> Result<(), RuntimeContractError> {
    if let Some(key) = extra.keys().find(|key| reserved.contains(&key.as_str())) {
        return Err(RuntimeContractError::ConflictingExtension(key.clone()));
    }
    Ok(())
}
