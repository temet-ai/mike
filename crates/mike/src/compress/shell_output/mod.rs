mod detect;
mod policy;
mod reason;
mod reduction;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_detect;

use std::collections::HashSet;

use bytes::Bytes;
use serde_json::Value;

use crate::compress::file_reads::splice::{
    PendingSplice, build_candidate, enforce_read_tool_result_boundary, resolve_overlaps,
};
use crate::compress::manifest::{StageEntry, StageOutcome, TargetEntry, sha256_hex};

use detect::{ShellContext, inspect_tool_result};
use policy::{POLICY_ID, policy_value};

pub(crate) use policy::{
    DEFAULT_GRACE_TURNS, DEFAULT_HEAD_LINES, DEFAULT_MIN_RESULT_BYTES,
    DEFAULT_SIGNAL_CONTEXT_LINES, DEFAULT_SIGNAL_LINE_CAP, DEFAULT_TAIL_LINES, POLICY_VERSION,
    PRODUCER, ShellOutputLimits, ShellOutputPolicy,
};
#[cfg(test)]
pub(crate) use reduction::REDUCTION_MARKER_TAG;

const STAGE_NAME: &str = "compression.shell_output";

pub(crate) struct ShellOutputReduction {
    pub(crate) candidate: Bytes,
    pub(crate) stage: StageEntry,
}

fn bash_tool_use_ids(messages: &[Value]) -> HashSet<&str> {
    messages
        .iter()
        .filter_map(|m| m.get("content").and_then(Value::as_array))
        .flatten()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
        .filter(|b| b.get("name").and_then(Value::as_str) == Some(PRODUCER))
        .filter_map(|b| b.get("id").and_then(Value::as_str))
        .collect()
}

fn assistant_turns_after(messages: &[Value]) -> Vec<usize> {
    let mut after = vec![0; messages.len()];
    let mut seen = 0;
    for (index, message) in messages.iter().enumerate().rev() {
        after[index] = seen;
        if message.get("role").and_then(Value::as_str) == Some("assistant") {
            seen += 1;
        }
    }
    after
}

fn detect_targets(
    messages: &[Value],
    ctx: &ShellContext<'_>,
) -> (Vec<TargetEntry>, Vec<PendingSplice>) {
    let mut targets = Vec::new();
    let mut pending = Vec::new();
    for (msg_idx, message) in messages.iter().enumerate() {
        let Some(blocks) = message.get("content").and_then(Value::as_array) else {
            continue;
        };
        for (block_idx, block) in blocks.iter().enumerate() {
            if block.get("type").and_then(Value::as_str) == Some("tool_result") {
                inspect_tool_result(block, (msg_idx, block_idx), ctx, &mut targets, &mut pending);
            }
        }
    }
    (targets, pending)
}

fn summarize(targets: &[TargetEntry]) -> (StageOutcome, &'static str) {
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

fn stage_entry(
    policy: &ShellOutputPolicy<'_>,
    (input_sha256, output_sha256): (String, String),
    outcome: StageOutcome,
    reason_code: &str,
    targets: Vec<TargetEntry>,
) -> StageEntry {
    StageEntry {
        ordinal: 1,
        stage: STAGE_NAME.to_string(),
        outcome,
        policy_id: POLICY_ID.to_string(),
        policy_version: POLICY_VERSION,
        resolved_parameters: policy_value(policy),
        input_sha256,
        output_sha256,
        targets,
        reason_code: reason_code.to_string(),
        detail: None,
    }
}

fn unchanged(
    raw: &[u8],
    policy: &ShellOutputPolicy<'_>,
    reason_code: &str,
) -> ShellOutputReduction {
    let sha = sha256_hex(raw);
    let outcome = if reason_code == reason::STAGE_MALFORMED_BODY {
        StageOutcome::Skipped
    } else {
        StageOutcome::Unchanged
    };
    ShellOutputReduction {
        candidate: Bytes::copy_from_slice(raw),
        stage: stage_entry(policy, (sha.clone(), sha), outcome, reason_code, Vec::new()),
    }
}

pub(crate) fn reduce_shell_output(
    raw: &[u8],
    policy: &ShellOutputPolicy<'_>,
) -> ShellOutputReduction {
    let Ok(body) = serde_json::from_slice::<Value>(raw) else {
        return unchanged(raw, policy, reason::STAGE_MALFORMED_BODY);
    };
    let Some(messages) = body.get("messages").and_then(Value::as_array) else {
        return unchanged(raw, policy, reason::STAGE_NO_CANDIDATES);
    };
    let bash_ids = bash_tool_use_ids(messages);
    if bash_ids.is_empty() {
        return unchanged(raw, policy, reason::STAGE_NO_CANDIDATES);
    }
    let assistant_after = assistant_turns_after(messages);
    let ctx = ShellContext {
        raw,
        bash_ids: &bash_ids,
        policy,
        newest_message: messages.len().saturating_sub(1),
        assistant_after: &assistant_after,
    };
    let (mut targets, mut pending) = detect_targets(messages, &ctx);
    resolve_overlaps(&mut targets, &mut pending);
    enforce_read_tool_result_boundary(
        raw,
        &body,
        &[PRODUCER.to_string()],
        &mut targets,
        &mut pending,
    );
    let candidate = build_candidate(raw, &pending, &mut targets);
    let shas = (sha256_hex(raw), sha256_hex(&candidate));
    let (outcome, reason_code) = summarize(&targets);
    ShellOutputReduction {
        candidate: Bytes::from(candidate),
        stage: stage_entry(policy, shas, outcome, reason_code, targets),
    }
}
