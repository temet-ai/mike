pub mod command;
pub mod detect;
pub mod policy;
pub mod reason;
pub mod reduction;

#[cfg(test)]
mod tests;
#[cfg(test)]
pub mod tests_detect;
#[cfg(test)]
mod tests_thread;

use std::collections::HashMap;

use bytes::Bytes;
use serde_json::Value;

use crate::compress::file_reads::detect::extract_content;
use crate::compress::file_reads::splice::{
    PendingSplice, build_candidate, enforce_read_tool_result_boundary, resolve_overlaps,
};
use crate::compress::history::conversation;
use crate::conversation::EarlierTurns;
use crate::wire::manifest::{StageEntry, StageOutcome, TargetEntry, sha256_hex};

use detect::{ShellContext, inspect_tool_result};
use policy::{POLICY_ID, policy_value};

pub use policy::{POLICY_VERSION, PRODUCER, ShellOutputLimits, ShellOutputPolicy};
#[cfg(test)]
pub use reduction::REDUCTION_MARKER_TAG;

const STAGE_NAME: &str = "compression.shell_output";

pub struct ShellOutputReduction {
    pub candidate: Bytes,
    pub stage: StageEntry,
}

fn bash_tool_use_ids(messages: &[Value]) -> HashMap<&str, &str> {
    messages
        .iter()
        .filter_map(|m| m.get("content").and_then(Value::as_array))
        .flatten()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
        .filter(|b| b.get("name").and_then(Value::as_str) == Some(PRODUCER))
        .filter_map(|b| {
            let id = b.get("id").and_then(Value::as_str)?;
            let command = b
                .pointer("/input/command")
                .and_then(Value::as_str)
                .unwrap_or("");
            Some((id, command))
        })
        .collect()
}

fn bash_results<'a>(
    messages: &'a [Value],
    bash_ids: &HashMap<&str, &str>,
) -> impl Iterator<Item = (usize, usize, &'a Value)> {
    messages
        .iter()
        .enumerate()
        .filter_map(|(m, message)| Some((m, message.get("content")?.as_array()?)))
        .flat_map(|(m, blocks)| {
            blocks
                .iter()
                .enumerate()
                .map(move |(b, block)| (m, b, block))
        })
        .filter(|(_, _, block)| block.get("type").and_then(Value::as_str) == Some("tool_result"))
        .filter(|(_, _, block)| {
            block
                .get("tool_use_id")
                .and_then(Value::as_str)
                .is_some_and(|id| bash_ids.contains_key(id))
        })
}

fn repeated_contents<'a>(
    messages: &'a [Value],
    bash_ids: &HashMap<&str, &str>,
) -> HashMap<&'a str, usize> {
    let mut repeats = HashMap::new();
    for (_, _, block) in bash_results(messages, bash_ids) {
        if let Some((content, _)) = extract_content(block, "") {
            *repeats.entry(content).or_default() += 1;
        }
    }
    repeats
}

fn detect_targets(
    messages: &[Value],
    ctx: &ShellContext<'_>,
) -> (Vec<TargetEntry>, Vec<PendingSplice>) {
    let mut targets = Vec::new();
    let mut pending = Vec::new();
    let mut seen = HashMap::new();
    for (msg_idx, block_idx, block) in bash_results(messages, ctx.bash_ids) {
        inspect_tool_result(
            block,
            (msg_idx, block_idx),
            ctx,
            &mut seen,
            &mut targets,
            &mut pending,
        );
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
    policy: &ShellOutputPolicy,
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

fn unchanged(raw: &[u8], policy: &ShellOutputPolicy, reason_code: &str) -> ShellOutputReduction {
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

pub fn reduce_shell_output(
    raw: &[u8],
    policy: &ShellOutputPolicy,
    earlier: &EarlierTurns,
) -> ShellOutputReduction {
    let Ok(body) = serde_json::from_slice::<Value>(raw) else {
        return unchanged(raw, policy, reason::STAGE_MALFORMED_BODY);
    };
    let Some(messages) = body.get("messages").and_then(Value::as_array) else {
        return unchanged(raw, policy, reason::STAGE_NO_CANDIDATES);
    };
    let history = conversation(messages, earlier);
    let bash_ids = bash_tool_use_ids(&history);
    if bash_ids.is_empty() {
        return unchanged(raw, policy, reason::STAGE_NO_CANDIDATES);
    }
    let repeats = repeated_contents(messages, &bash_ids);
    let ctx = ShellContext {
        raw,
        bash_ids: &bash_ids,
        policy,
        repeats: &repeats,
    };
    let (mut targets, mut pending) = detect_targets(messages, &ctx);
    resolve_overlaps(&mut targets, &mut pending);
    enforce_read_tool_result_boundary(
        raw,
        &body,
        &[PRODUCER.to_string()],
        earlier,
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
