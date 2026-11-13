//! Facade: `runtime` as mike_base sees it (crate split, ADR 0002).

pub use identity::{ProviderThreadRef, RuntimeKind, RuntimeNamespace, RuntimeThreadBinding};
pub use identity::{RuntimeContractError, validate_extensions, validate_text, validate_uuid};

pub mod identity;
