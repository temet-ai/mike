use serde_json::Value;

use crate::wire::manifest::{ByteRange, StageOutcome, TargetEntry, sha256_hex};

use super::reason;

fn generic_content_hash(block: &Value) -> String {
    let content = block.get("content").cloned().unwrap_or(Value::Null);
    let bytes = serde_json::to_vec(&content).unwrap_or_default();
    sha256_hex(&bytes)
}

pub fn unpaired_target(
    block: &Value,
    base_path: &str,
    tool_use_id: Option<&str>,
) -> TargetEntry {
    let hash = generic_content_hash(block);
    TargetEntry {
        outcome: StageOutcome::Skipped,
        content_kind: "tool_result".to_string(),
        json_path: Some(base_path.to_string()),
        tool_use_id: tool_use_id.map(str::to_string),
        producer: None,
        input_range: ByteRange::UNLOCATED,
        output_range: ByteRange::UNLOCATED,
        original_bytes: 0,
        sent_bytes: 0,
        original_tokens: None,
        sent_tokens: None,
        original_content_sha256: hash.clone(),
        sent_content_sha256: hash,
        recovery_ref: None,
        reason_code: reason::UNPAIRED_RESULT.to_string(),
        detail: Some("no matching tool_use for this tool_result's tool_use_id".to_string()),
    }
}

pub fn non_string_content_target(
    block: &Value,
    base_path: &str,
    tool_use_id: &str,
    producer: &str,
) -> TargetEntry {
    let hash = generic_content_hash(block);
    TargetEntry {
        outcome: StageOutcome::Skipped,
        content_kind: "read_result_text".to_string(),
        json_path: Some(base_path.to_string()),
        tool_use_id: Some(tool_use_id.to_string()),
        producer: Some(producer.to_string()),
        input_range: ByteRange::UNLOCATED,
        output_range: ByteRange::UNLOCATED,
        original_bytes: 0,
        sent_bytes: 0,
        original_tokens: None,
        sent_tokens: None,
        original_content_sha256: hash.clone(),
        sent_content_sha256: hash,
        recovery_ref: None,
        reason_code: reason::NON_STRING_CONTENT.to_string(),
        detail: Some("content is neither a bare string nor a single text part".to_string()),
    }
}

pub struct ClosedTargetParams<'a> {
    pub outcome: StageOutcome,
    pub reason_code: &'a str,
    pub detail: &'a str,
    pub json_path: &'a str,
    pub tool_use_id: &'a str,
    pub producer: &'a str,
    pub content_str: &'a str,
}

pub fn closed_target(params: ClosedTargetParams<'_>) -> TargetEntry {
    let hash = sha256_hex(params.content_str.as_bytes());
    let tokens = Some((params.content_str.len() / 4) as u64);
    TargetEntry {
        outcome: params.outcome,
        content_kind: "read_result_text".to_string(),
        json_path: Some(params.json_path.to_string()),
        tool_use_id: Some(params.tool_use_id.to_string()),
        producer: Some(params.producer.to_string()),
        input_range: ByteRange::UNLOCATED,
        output_range: ByteRange::UNLOCATED,
        original_bytes: params.content_str.len(),
        sent_bytes: params.content_str.len(),
        original_tokens: tokens,
        sent_tokens: tokens,
        original_content_sha256: hash.clone(),
        sent_content_sha256: hash,
        recovery_ref: None,
        reason_code: params.reason_code.to_string(),
        detail: Some(params.detail.to_string()),
    }
}
