pub mod detect;
pub mod glob;
pub mod markers;
pub mod outcome;
pub mod policy;
pub mod reason;
pub mod reduction;
pub mod splice;
pub mod supersede;
pub mod targets;

#[cfg(test)]
pub mod tests_basic_reduction;
#[cfg(test)]
pub mod tests_byte_identity;
#[cfg(test)]
pub mod tests_history;
#[cfg(test)]
pub mod tests_policy_v2;
#[cfg(test)]
pub mod tests_recovery;
#[cfg(test)]
pub mod tests_support;

use std::collections::HashMap;

use bytes::Bytes;
use serde_json::Value;

use crate::wire::manifest::{StageEntry, StageOutcome, TargetEntry, sha256_hex};

use detect::{ReadContext, inspect_tool_result};
use markers::collect_omitted_ranges;
use outcome::{summarize, unchanged_stage};
use policy::{POLICY_ID, POLICY_VERSION, policy_value};
use splice::{PendingSplice, build_candidate, enforce_read_tool_result_boundary, resolve_overlaps};

pub use policy::{
    DEFAULT_GRACE_TURNS, DEFAULT_HEAD_LINES, DEFAULT_MIN_RESULT_BYTES, DEFAULT_TAIL_LINES,
    FileReadPolicy,
};

const STAGE_NAME: &str = "compression.file_read";

pub struct FileReadReduction {
    pub candidate: Bytes,
    pub stage: StageEntry,
}

fn build_tool_use_index(messages: &[Value]) -> HashMap<&str, (&str, &Value)> {
    let mut use_by_id: HashMap<&str, (&str, &Value)> = HashMap::new();
    for message in messages {
        let Some(blocks) = message.get("content").and_then(Value::as_array) else {
            continue;
        };
        for block in blocks {
            if block.get("type").and_then(Value::as_str) != Some("tool_use") {
                continue;
            }
            let Some(id) = block.get("id").and_then(Value::as_str) else {
                continue;
            };
            let Some(name) = block.get("name").and_then(Value::as_str) else {
                continue;
            };
            let input = block.get("input").unwrap_or(&Value::Null);
            use_by_id.insert(id, (name, input));
        }
    }
    use_by_id
}

fn detect_targets(
    messages: &[Value],
    read_ctx: &ReadContext<'_>,
) -> (Vec<TargetEntry>, Vec<PendingSplice>) {
    let mut targets: Vec<TargetEntry> = Vec::new();
    let mut pending = Vec::new();
    for (msg_idx, message) in messages.iter().enumerate() {
        let Some(blocks) = message.get("content").and_then(Value::as_array) else {
            continue;
        };
        for (block_idx, block) in blocks.iter().enumerate() {
            if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                continue;
            }
            inspect_tool_result(
                block,
                msg_idx,
                block_idx,
                read_ctx,
                &mut targets,
                &mut pending,
            );
        }
    }
    (targets, pending)
}

pub fn reduce_file_reads(raw: &[u8], policy: &FileReadPolicy<'_>) -> FileReadReduction {
    let original_sha256 = sha256_hex(raw);

    let body: Value = match serde_json::from_slice(raw) {
        Ok(v) => v,
        Err(_) => {
            return unchanged_stage(
                raw,
                original_sha256,
                StageOutcome::Skipped,
                reason::STAGE_MALFORMED_BODY,
                "request body did not parse as JSON",
                policy,
            );
        }
    };
    if !body.is_object() {
        return unchanged_stage(
            raw,
            original_sha256,
            StageOutcome::Skipped,
            reason::STAGE_MALFORMED_BODY,
            "request body is not a JSON object",
            policy,
        );
    }

    let Some(messages) = body.get("messages").and_then(Value::as_array) else {
        return unchanged_stage(
            raw,
            original_sha256,
            StageOutcome::Unchanged,
            reason::STAGE_NO_CANDIDATES,
            "no messages array in the body",
            policy,
        );
    };

    let use_by_id = build_tool_use_index(messages);
    let omitted = collect_omitted_ranges(messages);
    let assistant_after = supersede::assistant_turns_after(messages);
    let superseded = supersede::superseded_reads(messages, policy.enabled_producers);
    let stale = supersede::stale_reads(messages, policy.enabled_producers);
    let read_ctx = ReadContext {
        raw,
        use_by_id: &use_by_id,
        policy,
        newest_message: messages.len().saturating_sub(1),
        omitted: &omitted,
        assistant_after: &assistant_after,
        superseded: &superseded,
        stale: &stale,
    };
    let (mut targets, mut pending) = detect_targets(messages, &read_ctx);

    resolve_overlaps(&mut targets, &mut pending);
    enforce_read_tool_result_boundary(
        raw,
        &body,
        policy.enabled_producers,
        &mut targets,
        &mut pending,
    );
    let candidate = build_candidate(raw, &pending, &mut targets);
    let output_sha256 = sha256_hex(&candidate);
    let (stage_outcome, stage_reason) = summarize(&targets);

    FileReadReduction {
        candidate: Bytes::from(candidate),
        stage: StageEntry {
            ordinal: 1,
            stage: STAGE_NAME.to_string(),
            outcome: stage_outcome,
            policy_id: POLICY_ID.to_string(),
            policy_version: POLICY_VERSION,
            resolved_parameters: policy_value(policy),
            input_sha256: original_sha256,
            output_sha256,
            targets,
            reason_code: stage_reason.to_string(),
            detail: None,
        },
    }
}
