use bytes::Bytes;

use crate::compress::manifest::{StageEntry, StageOutcome, TargetEntry};

use super::policy::{FileReadPolicy, POLICY_ID, POLICY_VERSION, policy_value};
use super::reason;
use super::{FileReadReduction, STAGE_NAME};

pub(super) fn summarize(targets: &[TargetEntry]) -> (StageOutcome, &'static str) {
    if targets.iter().any(|t| t.outcome == StageOutcome::Applied) {
        return (StageOutcome::Applied, reason::STAGE_APPLIED);
    }
    if !targets.is_empty()
        && targets
            .iter()
            .all(|t| t.outcome == StageOutcome::FailedClosed)
    {
        return (StageOutcome::FailedClosed, reason::STAGE_ALL_FAILED_CLOSED);
    }
    (StageOutcome::Unchanged, reason::STAGE_NO_CANDIDATES)
}

pub(super) fn unchanged_stage(
    raw: &[u8],
    original_sha256: String,
    outcome: StageOutcome,
    reason_code: &str,
    detail: &str,
    policy: &FileReadPolicy<'_>,
) -> FileReadReduction {
    FileReadReduction {
        candidate: Bytes::copy_from_slice(raw),
        stage: StageEntry {
            ordinal: 1,
            stage: STAGE_NAME.to_string(),
            outcome,
            policy_id: POLICY_ID.to_string(),
            policy_version: POLICY_VERSION,
            resolved_parameters: policy_value(policy),
            input_sha256: original_sha256.clone(),
            output_sha256: original_sha256,
            targets: Vec::new(),
            reason_code: reason_code.to_string(),
            detail: Some(detail.to_string()),
        },
    }
}
