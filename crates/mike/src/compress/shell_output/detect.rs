use std::collections::HashSet;

use serde_json::Value;

use crate::compress::file_reads::detect::{extract_content, find_subsequence};
use crate::compress::file_reads::splice::PendingSplice;
use crate::wire::manifest::{ByteRange, StageOutcome, TargetEntry, sha256_hex};

use super::policy::{PRODUCER, ShellOutputPolicy};
use super::reason;
use super::reduction::{REDUCTION_MARKER_TAG, build_reduction};

const CONTENT_KIND: &str = "shell_result_text";
const INJECTED_CONTENT_MARKERS: [&str; 2] = ["<system-reminder", "[[mike:"];

pub(super) struct ShellContext<'a> {
    pub(super) raw: &'a [u8],
    pub(super) bash_ids: &'a HashSet<&'a str>,
    pub(super) policy: &'a ShellOutputPolicy<'a>,
    pub(super) newest_message: usize,
    pub(super) assistant_after: &'a [usize],
}

struct Candidate<'a> {
    json_path: String,
    tool_use_id: &'a str,
    content: &'a str,
}

fn skipped(candidate: &Candidate<'_>, reason_code: &str, detail: String) -> TargetEntry {
    let hash = sha256_hex(candidate.content.as_bytes());
    let tokens = Some((candidate.content.len() / 4) as u64);
    TargetEntry {
        outcome: StageOutcome::Skipped,
        content_kind: CONTENT_KIND.to_string(),
        json_path: Some(candidate.json_path.clone()),
        tool_use_id: Some(candidate.tool_use_id.to_string()),
        producer: Some(PRODUCER.to_string()),
        input_range: ByteRange::UNLOCATED,
        output_range: ByteRange::UNLOCATED,
        original_bytes: candidate.content.len(),
        sent_bytes: candidate.content.len(),
        original_tokens: tokens,
        sent_tokens: tokens,
        original_content_sha256: hash.clone(),
        sent_content_sha256: hash,
        recovery_ref: None,
        reason_code: reason_code.to_string(),
        detail: Some(detail),
    }
}

fn content_exemption(
    block: &Value,
    content: &str,
    ctx: &ShellContext<'_>,
) -> Option<(&'static str, String)> {
    if content.contains(REDUCTION_MARKER_TAG) {
        return Some((
            reason::ALREADY_REDUCED,
            "content already carries a shell-output reduction marker".to_string(),
        ));
    }
    if let Some(marker) = INJECTED_CONTENT_MARKERS
        .iter()
        .find(|m| content.contains(**m))
    {
        return Some((
            reason::INJECTED_CONTENT,
            format!(
                "content carries {marker}, text a hook, mod, tether or mike added; it stays whole"
            ),
        ));
    }
    if block.get("is_error").and_then(Value::as_bool) == Some(true) {
        return Some((
            reason::ERROR_RESULT,
            "tool_result.is_error is true".to_string(),
        ));
    }
    let floor = ctx.policy.limits.min_result_bytes;
    if content.len() < floor {
        return Some((
            reason::SMALL_RESULT,
            format!(
                "content is {} bytes, below the {floor}-byte floor",
                content.len()
            ),
        ));
    }
    None
}

fn history_exemption(msg_idx: usize, ctx: &ShellContext<'_>) -> Option<(&'static str, String)> {
    if msg_idx >= ctx.newest_message {
        return Some((
            reason::FRESHEST_RESULT,
            "the result is in the newest message; the model has not seen it yet".to_string(),
        ));
    }
    let turns = ctx.assistant_after.get(msg_idx).copied().unwrap_or(0);
    let grace = ctx.policy.limits.grace_turns;
    if turns < grace {
        return Some((
            reason::GRACE_WINDOW,
            format!(
                "the model has answered {turns} time(s) since this output; it stays whole for {grace} answers"
            ),
        ));
    }
    None
}

fn locate(raw: &[u8], needle: &[u8]) -> Result<usize, (&'static str, String)> {
    let Some(first) = find_subsequence(raw, needle) else {
        return Err((
            reason::RANGE_NOT_FOUND,
            "the output's JSON-encoded bytes were not found verbatim in the body".to_string(),
        ));
    };
    if find_subsequence(&raw[first + needle.len()..], needle).is_some() {
        return Err((
            reason::RANGE_AMBIGUOUS,
            "the output's JSON-encoded bytes appear more than once in the body".to_string(),
        ));
    }
    Ok(first)
}

fn reduced_text(
    candidate: &Candidate<'_>,
    recovery_ref: &str,
    ctx: &ShellContext<'_>,
) -> Result<(String, String), (&'static str, String)> {
    let Some(reduction) = build_reduction(candidate.content, &ctx.policy.shape()) else {
        return Err((
            reason::NO_SHRINK,
            "the output has no lines outside the kept head, tail and signal lines".to_string(),
        ));
    };
    let text = format!(
        "{}\nExact captured original: call get_request_body with {recovery_ref}. This returns the whole output as the command printed it.",
        reduction.text
    );
    if !ctx
        .policy
        .shrinks_enough(candidate.content.len(), text.len())
    {
        return Err((
            reason::NO_SHRINK,
            format!(
                "reduced form is {} of {} bytes; it must save at least {}%",
                text.len(),
                candidate.content.len(),
                ctx.policy.limits.min_shrink_percent
            ),
        ));
    }
    let detail = format!(
        "kept output lines {} ({} of {} lines, {} signal lines) in place of {} omitted range(s); the exact original is recoverable",
        reduction.kept_ranges,
        reduction.kept_lines,
        reduction.result_lines,
        reduction.signal_lines,
        reduction.omitted_ranges
    );
    Ok((text, detail))
}

fn attempt(
    candidate: &Candidate<'_>,
    recovery_ref: String,
    ctx: &ShellContext<'_>,
) -> Result<(TargetEntry, PendingSplice), (&'static str, String)> {
    let (text, detail) = reduced_text(candidate, &recovery_ref, ctx)?;
    let encode_failed = |e: serde_json::Error| {
        (
            reason::ENCODE_FAILED,
            format!("failed to JSON-encode the output: {e}"),
        )
    };
    let original_encoded = serde_json::to_vec(candidate.content).map_err(encode_failed)?;
    let new_bytes = serde_json::to_vec(&text).map_err(encode_failed)?;
    let start = locate(ctx.raw, &original_encoded)?;
    let input_range = ByteRange {
        start,
        end: start + original_encoded.len(),
    };
    let target = TargetEntry {
        outcome: StageOutcome::Applied,
        content_kind: CONTENT_KIND.to_string(),
        json_path: Some(candidate.json_path.clone()),
        tool_use_id: Some(candidate.tool_use_id.to_string()),
        producer: Some(PRODUCER.to_string()),
        input_range,
        output_range: ByteRange::UNLOCATED,
        original_bytes: candidate.content.len(),
        sent_bytes: text.len(),
        original_tokens: Some((candidate.content.len() / 4) as u64),
        sent_tokens: Some((text.len() / 4) as u64),
        original_content_sha256: sha256_hex(candidate.content.as_bytes()),
        sent_content_sha256: sha256_hex(text.as_bytes()),
        recovery_ref: Some(recovery_ref),
        reason_code: reason::SHELL_OUTPUT_REDUCED.to_string(),
        detail: Some(detail),
    };
    let splice = PendingSplice {
        start: input_range.start,
        end: input_range.end,
        new_bytes,
        target_index: 0,
    };
    Ok((target, splice))
}

fn exemption(
    block: &Value,
    msg_idx: usize,
    candidate: &Candidate<'_>,
    ctx: &ShellContext<'_>,
) -> Option<(&'static str, String)> {
    content_exemption(block, candidate.content, ctx)
        .or_else(|| history_exemption(msg_idx, ctx))
        .or_else(|| {
            ctx.policy.holds_for_warm_cache(candidate.tool_use_id).then(|| {
                (
                    reason::CACHE_WARM,
                    "the conversation's prompt cache is warm and this output was not shortened before; shortening it now would re-bill every later block as a cache write".to_string(),
                )
            })
        })
}

pub(super) fn inspect_tool_result(
    block: &Value,
    (msg_idx, block_idx): (usize, usize),
    ctx: &ShellContext<'_>,
    targets: &mut Vec<TargetEntry>,
    pending: &mut Vec<PendingSplice>,
) {
    let Some(tool_use_id) = block.get("tool_use_id").and_then(Value::as_str) else {
        return;
    };
    if !ctx.bash_ids.contains(tool_use_id) {
        return;
    }
    let base_path = format!("/messages/{msg_idx}/content/{block_idx}");
    let Some((content, json_path)) = extract_content(block, &base_path) else {
        return;
    };
    let candidate = Candidate {
        json_path,
        tool_use_id,
        content,
    };
    if let Some((code, detail)) = exemption(block, msg_idx, &candidate, ctx) {
        targets.push(skipped(&candidate, code, detail));
        return;
    }
    let Some(recovery_ref) = ctx
        .policy
        .capture
        .as_ref()
        .and_then(|c| c.reference(tool_use_id, content))
    else {
        targets.push(skipped(
            &candidate,
            reason::ORIGINAL_NOT_RECOVERABLE,
            "the original captured output does not uniquely match this content".to_string(),
        ));
        return;
    };
    match attempt(&candidate, recovery_ref, ctx) {
        Ok((target, mut splice)) => {
            splice.target_index = targets.len();
            targets.push(target);
            pending.push(splice);
        }
        Err((code, detail)) => {
            let outcome = if code == reason::NO_SHRINK {
                StageOutcome::Skipped
            } else {
                StageOutcome::FailedClosed
            };
            let mut target = skipped(&candidate, code, detail);
            target.outcome = outcome;
            targets.push(target);
        }
    }
}
