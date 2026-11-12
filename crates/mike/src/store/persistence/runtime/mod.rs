pub mod codex;
pub mod diagnostics;
mod events;
mod identity;
pub(super) mod identity_budget;
mod notification_targets;
mod notifications;
mod observation;
mod observation_budget;
mod observation_reads;
mod observation_write;
mod retention;

pub use notification_targets::RuntimeNotificationTarget;
pub use observation::{ObservationApplyOutcome, ObservationLimits};
pub use observation_reads::RuntimeItemRow;
pub use observation_reads::SessionObservationRow;
pub use retention::RuntimeRetentionOutcome;

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

#[cfg(test)]
mod tests_observation;
#[cfg(test)]
mod tests_observation_support;

#[cfg(test)]
mod tests_diagnostics;
#[cfg(test)]
mod tests_observation_boundaries;
#[cfg(test)]
mod tests_observation_retention;
