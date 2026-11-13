use serde_json::Value;

use super::admission::{ExclusionReason, IntakeContext, exclusion};
use super::protocol::audit_routing;
use crate::runtime::{RuntimeContractError, RuntimeEventEnvelope, RuntimeObservation};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntakeResult {
    Captured(Box<RuntimeObservation>),
    Excluded(ExclusionReason),
}

pub fn observe(
    envelope: RuntimeEventEnvelope,
    raw_payload: Vec<u8>,
    context: IntakeContext<'_>,
) -> Result<IntakeResult, RuntimeContractError> {
    let parsed = serde_json::from_slice::<Value>(&raw_payload).ok();
    if let Some(reason) = exclusion(parsed.as_ref(), context, audit_routing(&raw_payload)) {
        return Ok(IntakeResult::Excluded(reason));
    }
    RuntimeObservation::new(envelope, raw_payload)
        .map(|observation| IntakeResult::Captured(Box::new(observation)))
}
