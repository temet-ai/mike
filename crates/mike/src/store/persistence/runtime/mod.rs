pub mod codex;
mod events;
mod identity;

use super::PersistError;
use crate::runtime::RuntimeContractError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeWriteOutcome {
    Inserted,
    Unchanged,
}

fn invalid(error: RuntimeContractError) -> PersistError {
    PersistError::DataInvariant(error.to_string())
}

fn same_or_conflict<T: PartialEq>(
    existing: Option<T>,
    expected: &T,
    kind: &str,
) -> Result<RuntimeWriteOutcome, PersistError> {
    if existing.as_ref() == Some(expected) {
        return Ok(RuntimeWriteOutcome::Unchanged);
    }
    Err(PersistError::DataInvariant(format!(
        "runtime {kind} conflicts with an existing identity or references a missing Mike session"
    )))
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_events;
#[cfg(test)]
mod tests_identity;
