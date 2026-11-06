pub mod codex;
mod events;
mod identity;

pub use events::{RuntimeEventEnvelope, RuntimeEventSource, RuntimeObservation};
pub use identity::{ProviderThreadRef, RuntimeKind, RuntimeNamespace, RuntimeThreadBinding};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RuntimeContractError {
    #[error("runtime field {0} must be nonempty and contain no NUL characters")]
    InvalidText(&'static str),
    #[error("runtime field {0} must not be a nil UUID")]
    InvalidUuid(&'static str),
    #[error("Codex managed fleet launch is not supported yet; choose runtime = \"claude\"")]
    UnsupportedManagedRuntime,
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

pub(crate) fn validate_text(value: &str, field: &'static str) -> Result<(), RuntimeContractError> {
    if value.trim().is_empty() || value.contains('\0') {
        return Err(RuntimeContractError::InvalidText(field));
    }
    Ok(())
}

pub(crate) fn validate_uuid(
    value: uuid::Uuid,
    field: &'static str,
) -> Result<(), RuntimeContractError> {
    if value.is_nil() {
        return Err(RuntimeContractError::InvalidUuid(field));
    }
    Ok(())
}

pub(crate) fn validate_extensions(
    extra: &std::collections::BTreeMap<String, serde_json::Value>,
    reserved: &[&str],
) -> Result<(), RuntimeContractError> {
    if let Some(key) = extra.keys().find(|key| reserved.contains(&key.as_str())) {
        return Err(RuntimeContractError::ConflictingExtension(key.clone()));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
