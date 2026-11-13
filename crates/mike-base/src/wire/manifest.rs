use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageOutcome {
    Applied,
    Unchanged,
    Skipped,
    FailedClosed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

impl ByteRange {
    pub const UNLOCATED: ByteRange = ByteRange { start: 0, end: 0 };
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TargetEntry {
    pub outcome: StageOutcome,
    pub content_kind: String,
    pub json_path: Option<String>,
    pub tool_use_id: Option<String>,
    pub producer: Option<String>,
    pub input_range: ByteRange,
    pub output_range: ByteRange,
    pub original_bytes: usize,
    pub sent_bytes: usize,
    pub original_tokens: Option<u64>,
    pub sent_tokens: Option<u64>,
    pub original_content_sha256: String,
    pub sent_content_sha256: String,
    pub recovery_ref: Option<String>,
    pub reason_code: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StageEntry {
    pub ordinal: u32,
    pub stage: String,
    pub outcome: StageOutcome,
    pub policy_id: String,
    pub policy_version: u32,
    pub resolved_parameters: serde_json::Value,
    pub input_sha256: String,
    pub output_sha256: String,
    pub targets: Vec<TargetEntry>,
    pub reason_code: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransformationManifest {
    pub schema_version: u32,
    pub original_sha256: String,
    pub sent_sha256: String,
    pub stages: Vec<StageEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ChainError {
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
    pub fn verify_chain(&self) -> Result<(), ChainError> {
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

pub struct AppliedStage<'a> {
    pub stage: &'a str,
    pub policy_id: &'a str,
    pub policy_version: u32,
    pub resolved_parameters: serde_json::Value,
    pub new_bytes: &'a [u8],
    pub targets: Vec<TargetEntry>,
    pub reason_code: &'a str,
    pub detail: Option<String>,
}

pub struct UnchangedStage<'a> {
    pub stage: &'a str,
    pub policy_id: &'a str,
    pub reason_code: &'a str,
    pub resolved_parameters: serde_json::Value,
    pub targets: Vec<TargetEntry>,
    pub detail: Option<String>,
}

pub struct ManifestBuilder {
    original_sha256: String,
    current_sha256: String,
    stages: Vec<StageEntry>,
    next_ordinal: u32,
}

impl ManifestBuilder {
    pub fn new(original_sha256: String) -> Self {
        Self {
            current_sha256: original_sha256.clone(),
            original_sha256,
            stages: Vec::new(),
            next_ordinal: 1,
        }
    }

    pub fn record_applied(&mut self, applied: AppliedStage<'_>) {
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

    pub fn applied(&self, stage: &str) -> bool {
        self.stages
            .iter()
            .any(|entry| entry.stage == stage && entry.outcome == StageOutcome::Applied)
    }

    pub fn record_unchanged(&mut self, stage: &str, policy_id: &str, reason_code: &str) {
        self.record_unchanged_with(UnchangedStage {
            stage,
            policy_id,
            reason_code,
            resolved_parameters: serde_json::json!({}),
            targets: Vec::new(),
            detail: None,
        });
    }

    pub fn record_unchanged_with(&mut self, unchanged: UnchangedStage<'_>) {
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

    pub fn finish(self, sent_sha256: String) -> TransformationManifest {
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
pub mod manifest_tests;
