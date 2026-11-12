use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(crate) const MANIFEST_SCHEMA_VERSION: u32 = 1;

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StageOutcome {
    Applied,
    Unchanged,
    Skipped,
    FailedClosed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ByteRange {
    pub(crate) start: usize,
    pub(crate) end: usize,
}

impl ByteRange {
    pub(crate) const UNLOCATED: ByteRange = ByteRange { start: 0, end: 0 };
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct TargetEntry {
    pub(crate) outcome: StageOutcome,
    pub(crate) content_kind: String,
    pub(crate) json_path: Option<String>,
    pub(crate) tool_use_id: Option<String>,
    pub(crate) producer: Option<String>,
    pub(crate) input_range: ByteRange,
    pub(crate) output_range: ByteRange,
    pub(crate) original_bytes: usize,
    pub(crate) sent_bytes: usize,
    pub(crate) original_tokens: Option<u64>,
    pub(crate) sent_tokens: Option<u64>,
    pub(crate) original_content_sha256: String,
    pub(crate) sent_content_sha256: String,
    pub(crate) recovery_ref: Option<String>,
    pub(crate) reason_code: String,
    pub(crate) detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct StageEntry {
    pub(crate) ordinal: u32,
    pub(crate) stage: String,
    pub(crate) outcome: StageOutcome,
    pub(crate) policy_id: String,
    pub(crate) policy_version: u32,
    pub(crate) resolved_parameters: serde_json::Value,
    pub(crate) input_sha256: String,
    pub(crate) output_sha256: String,
    pub(crate) targets: Vec<TargetEntry>,
    pub(crate) reason_code: String,
    pub(crate) detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct TransformationManifest {
    pub(crate) schema_version: u32,
    pub(crate) original_sha256: String,
    pub(crate) sent_sha256: String,
    pub(crate) stages: Vec<StageEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ChainError {
    #[error("stage {ordinal} input_sha256 does not match the manifest's original_sha256")]
    FirstStageInputMismatch { ordinal: u32 },
    #[error("stage {ordinal} input_sha256 does not match the previous stage's output_sha256")]
    StageChainBroken { ordinal: u32 },
    #[error("the last stage's ({ordinal}) output_sha256 does not match the manifest's sent_sha256")]
    LastStageOutputMismatch { ordinal: u32 },
    #[error("manifest has no stages but original_sha256 != sent_sha256")]
    EmptyStagesBodyChanged,
}

impl TransformationManifest {
    pub(crate) fn verify_chain(&self) -> Result<(), ChainError> {
        if self.stages.is_empty() {
            return if self.original_sha256 == self.sent_sha256 {
                Ok(())
            } else {
                Err(ChainError::EmptyStagesBodyChanged)
            };
        }

        let mut expected_input: &str = &self.original_sha256;
        let mut last_ordinal = 0u32;
        for (i, stage) in self.stages.iter().enumerate() {
            if stage.input_sha256 != expected_input {
                return Err(if i == 0 {
                    ChainError::FirstStageInputMismatch {
                        ordinal: stage.ordinal,
                    }
                } else {
                    ChainError::StageChainBroken {
                        ordinal: stage.ordinal,
                    }
                });
            }
            expected_input = &stage.output_sha256;
            last_ordinal = stage.ordinal;
        }

        if expected_input != self.sent_sha256 {
            return Err(ChainError::LastStageOutputMismatch {
                ordinal: last_ordinal,
            });
        }
        Ok(())
    }
}

pub(crate) struct AppliedStage<'a> {
    pub(crate) stage: &'a str,
    pub(crate) policy_id: &'a str,
    pub(crate) policy_version: u32,
    pub(crate) resolved_parameters: serde_json::Value,
    pub(crate) new_bytes: &'a [u8],
    pub(crate) targets: Vec<TargetEntry>,
    pub(crate) reason_code: &'a str,
    pub(crate) detail: Option<String>,
}

pub(crate) struct UnchangedStage<'a> {
    pub(crate) stage: &'a str,
    pub(crate) policy_id: &'a str,
    pub(crate) reason_code: &'a str,
    pub(crate) resolved_parameters: serde_json::Value,
    pub(crate) targets: Vec<TargetEntry>,
    pub(crate) detail: Option<String>,
}

pub(crate) struct ManifestBuilder {
    original_sha256: String,
    current_sha256: String,
    stages: Vec<StageEntry>,
    next_ordinal: u32,
}

impl ManifestBuilder {
    pub(crate) fn new(original_sha256: String) -> Self {
        Self {
            current_sha256: original_sha256.clone(),
            original_sha256,
            stages: Vec::new(),
            next_ordinal: 1,
        }
    }

    pub(crate) fn record_applied(&mut self, applied: AppliedStage<'_>) {
        let output_sha256 = sha256_hex(applied.new_bytes);
        self.stages.push(StageEntry {
            ordinal: self.next_ordinal,
            stage: applied.stage.to_string(),
            outcome: StageOutcome::Applied,
            policy_id: applied.policy_id.to_string(),
            policy_version: applied.policy_version,
            resolved_parameters: applied.resolved_parameters,
            input_sha256: self.current_sha256.clone(),
            output_sha256: output_sha256.clone(),
            targets: applied.targets,
            reason_code: applied.reason_code.to_string(),
            detail: applied.detail,
        });
        self.next_ordinal += 1;
        self.current_sha256 = output_sha256;
    }

    pub(crate) fn applied(&self, stage: &str) -> bool {
        self.stages
            .iter()
            .any(|entry| entry.stage == stage && entry.outcome == StageOutcome::Applied)
    }

    pub(crate) fn record_unchanged(&mut self, stage: &str, policy_id: &str, reason_code: &str) {
        self.record_unchanged_with(UnchangedStage {
            stage,
            policy_id,
            reason_code,
            resolved_parameters: serde_json::json!({}),
            targets: Vec::new(),
            detail: None,
        });
    }

    pub(crate) fn record_unchanged_with(&mut self, unchanged: UnchangedStage<'_>) {
        self.stages.push(StageEntry {
            ordinal: self.next_ordinal,
            stage: unchanged.stage.to_string(),
            outcome: StageOutcome::Unchanged,
            policy_id: unchanged.policy_id.to_string(),
            policy_version: 1,
            resolved_parameters: unchanged.resolved_parameters,
            input_sha256: self.current_sha256.clone(),
            output_sha256: self.current_sha256.clone(),
            targets: unchanged.targets,
            reason_code: unchanged.reason_code.to_string(),
            detail: unchanged.detail,
        });
        self.next_ordinal += 1;
    }

    pub(crate) fn finish(self, sent_sha256: String) -> TransformationManifest {
        TransformationManifest {
            schema_version: MANIFEST_SCHEMA_VERSION,
            original_sha256: self.original_sha256,
            sent_sha256,
            stages: self.stages,
        }
    }
}

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod manifest_tests;
