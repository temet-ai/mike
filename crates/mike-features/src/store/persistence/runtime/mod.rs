pub use mike_base::store::persistence::runtime::*;
pub mod codex;
pub mod diagnostics;
pub mod events;
pub mod identity;
pub mod notification_targets;
pub mod notifications;
pub mod observation;
pub mod observation_budget;
pub mod observation_reads;
pub mod observation_write;
pub mod retention;

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
pub mod tests_events;
#[cfg(test)]
pub mod tests_identity;

#[cfg(test)]
pub mod tests_observation_support;

#[cfg(test)]
pub mod tests_diagnostics;
#[cfg(test)]
pub mod tests_observation_boundaries;
#[cfg(test)]
pub mod tests_observation_retention;
