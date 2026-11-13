use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::wire::manifest::{ByteRange, StageOutcome, TargetEntry, sha256_hex};

use super::markers::{OmittedRange, is_recovery_read, read_offset};
use super::policy::FileReadPolicy;
use super::reason;
use super::reduction::{ReductionInput, build_reduction};
use super::splice::PendingSplice;
use super::supersede::{SUPERSEDED_MARKER_TAG, superseded_text};
use super::targets::{
    ClosedTargetParams, closed_target, non_string_content_target, unpaired_target,
};

pub const REDUCTION_MARKER_TAG: &str = "[[mike:file_read_reduced]]";

pub struct ReadContext<'a> {
    pub raw: &'a [u8],
    pub use_by_id: &'a HashMap<&'a str, (&'a str, &'a Value)>,
    pub policy: &'a FileReadPolicy<'a>,
    pub newest_message: usize,
    pub omitted: &'a [OmittedRange],
    pub assistant_after: &'a [usize],
    pub superseded: &'a HashSet<&'a str>,
    pub stale: &'a HashSet<&'a str>,
}

struct TargetFields<'a> {
    json_path: &'a str,
    tool_use_id: &'a str,
    producer: &'a str,
    content_str: &'a str,
}

fn reject(
    targets: &mut Vec<TargetEntry>,
    outcome: StageOutcome,
    reason_code: &str,
    detail: &str,
    fields: &TargetFields<'_>,
) {
    targets.push(closed_target(ClosedTargetParams {
        outcome,
        reason_code,
        detail,
        json_path: fields.json_path,
        tool_use_id: fields.tool_use_id,
        producer: fields.producer,
        content_str: fields.content_str,
    }));
}

fn pre_filter_rejection(
    block: &Value,
    file_path: &str,
    content_str: &str,
    policy: &FileReadPolicy<'_>,
) -> Option<(&'static str, String)> {
    if policy.is_protected(file_path) {
        return Some((
            reason::PROTECTED_PATH,
            format!(
                "file_path {file_path} matches an operator-configured protected-path pattern; never reduced regardless of size"
            ),
        ));
    }
    if content_str.contains(REDUCTION_MARKER_TAG) || content_str.contains(SUPERSEDED_MARKER_TAG) {
        return Some((
            reason::ALREADY_REDUCED,
            "content already carries a reduction marker".to_string(),
        ));
    }
    if block.get("is_error").and_then(Value::as_bool) == Some(true) {
        return Some((
            reason::ERROR_RESULT,
            "tool_result.is_error is true".to_string(),
        ));
    }
    if content_str.len() < policy.min_result_bytes {
        return Some((
            reason::SMALL_READ,
            format!(
                "content is {} bytes, below the {}-byte floor",
                content_str.len(),
                policy.min_result_bytes
            ),
        ));
    }
    None
}

fn history_exemption(
    msg_idx: usize,
    input: &Value,
    fields: &TargetFields<'_>,
    file_path: &str,
    ctx: &ReadContext<'_>,
) -> Option<(&'static str, String)> {
    if msg_idx >= ctx.newest_message {
        return Some((
            reason::FRESHEST_RESULT,
            "the result is in the newest message; the model has not seen it yet".to_string(),
        ));
    }
    if is_recovery_read(input, file_path, ctx.omitted) {
        return Some((
            reason::RECOVERY_READ,
            format!(
                "offset {} of {file_path} lies in a range an earlier marker asked the model to re-read",
                read_offset(input)
            ),
        ));
    }
    if input.get("offset").is_some() || input.get("limit").is_some() {
        return Some((
            reason::RECOVERY_READ,
            "explicitly ranged reads remain whole, including when prior reduction markers exist only in sent history".to_string(),
        ));
    }
    let turns = ctx.assistant_after.get(msg_idx).copied().unwrap_or(0);
    if turns < ctx.policy.grace_turns
        && !ctx.stale.contains(fields.tool_use_id)
        && !ctx.superseded.contains(fields.tool_use_id)
    {
        return Some((
            reason::GRACE_WINDOW,
            format!(
                "the model has answered {turns} time(s) since this read of {file_path}; a read stays whole for {} answers",
                ctx.policy.grace_turns
            ),
        ));
    }
    None
}

fn history_or_warm_cache_exemption(
    msg_idx: usize,
    input: &Value,
    fields: &TargetFields<'_>,
    file_path: &str,
    ctx: &ReadContext<'_>,
) -> Option<(&'static str, String)> {
    history_exemption(msg_idx, input, fields, file_path, ctx).or_else(|| {
        ctx.policy.holds_for_warm_cache(fields.tool_use_id).then(|| {
            (
                reason::CACHE_WARM,
                "the conversation's prompt cache is warm and this read was not shortened before; shortening it now would re-bill every later block as a cache write".to_string(),
            )
        })
    })
}

fn push_attempt(
    result: Result<(TargetEntry, PendingSplice), ReduceRejection>,
    fields: &TargetFields<'_>,
    file_path: &str,
    targets: &mut Vec<TargetEntry>,
    pending: &mut Vec<PendingSplice>,
) {
    match result {
        Ok((target, mut splice)) => {
            splice.target_index = targets.len();
            targets.push(target);
            pending.push(splice);
        }
        Err(rejection) => {
            let detail = format!("file {file_path}: {}", rejection.detail);
            let mut t = closed_target(ClosedTargetParams {
                outcome: StageOutcome::FailedClosed,
                reason_code: rejection.reason_code,
                detail: &detail,
                json_path: fields.json_path,
                tool_use_id: fields.tool_use_id,
                producer: fields.producer,
                content_str: fields.content_str,
            });
            if let Some(range) = rejection.range {
                t.input_range = range;
                t.output_range = range;
            }
            targets.push(t);
        }
    }
}

pub fn inspect_tool_result<'a>(
    block: &'a Value,
    msg_idx: usize,
    block_idx: usize,
    ctx: &ReadContext<'a>,
    targets: &mut Vec<TargetEntry>,
    pending: &mut Vec<PendingSplice>,
) {
    let base_path = format!("/messages/{msg_idx}/content/{block_idx}");
    let Some(tool_use_id) = block.get("tool_use_id").and_then(Value::as_str) else {
        targets.push(unpaired_target(block, &base_path, None));
        return;
    };
    let Some(&(name, input)) = ctx.use_by_id.get(tool_use_id) else {
        targets.push(unpaired_target(block, &base_path, Some(tool_use_id)));
        return;
    };
    if !ctx.policy.is_enabled_producer(name) {
        return;
    }
    let Some(file_path) = input.get("file_path").and_then(Value::as_str) else {
        return;
    };
    let Some((content_str, content_json_path)) = extract_content(block, &base_path) else {
        targets.push(non_string_content_target(
            block,
            &base_path,
            tool_use_id,
            name,
        ));
        return;
    };

    let fields = TargetFields {
        json_path: &content_json_path,
        tool_use_id,
        producer: name,
        content_str,
    };
    if let Some((reason_code, detail)) =
        pre_filter_rejection(block, file_path, content_str, ctx.policy)
    {
        reject(
            targets,
            StageOutcome::Skipped,
            reason_code,
            &detail,
            &fields,
        );
        return;
    }
    let recovery_ref = ctx
        .policy
        .capture
        .as_ref()
        .and_then(|c| c.reference(tool_use_id, content_str));
    if ctx.policy.capture.is_some() && recovery_ref.is_none() {
        reject(
            targets,
            StageOutcome::Skipped,
            "original_not_recoverable",
            "the original captured result does not uniquely match this content",
            &fields,
        );
        return;
    }
    let attempt = ReduceAttempt {
        recovery_ref,
        raw: ctx.raw,
        content_str,
        file_path,
        producer: name,
        tool_use_id,
        content_json_path: &content_json_path,
        head_lines: ctx.policy.head_lines,
        tail_lines: ctx.policy.tail_lines,
        first_line: read_offset(input),
    };
    if let Some((reason_code, detail)) =
        history_or_warm_cache_exemption(msg_idx, input, &fields, file_path, ctx)
    {
        reject(
            targets,
            StageOutcome::Skipped,
            reason_code,
            &detail,
            &fields,
        );
        return;
    }
    if msg_idx < ctx.newest_message && ctx.superseded.contains(tool_use_id) {
        push_attempt(
            attempt_supersession(&attempt),
            &fields,
            file_path,
            targets,
            pending,
        );
        return;
    }
    push_attempt(
        attempt_reduction(&attempt),
        &fields,
        file_path,
        targets,
        pending,
    );
}

pub fn extract_content<'a>(
    block: &'a Value,
    base_path: &str,
) -> Option<(&'a str, String)> {
    match block.get("content") {
        Some(Value::String(s)) => Some((s.as_str(), format!("{base_path}/content"))),
        Some(Value::Array(parts)) if parts.len() == 1 => {
            let part = &parts[0];
            if part.get("type").and_then(Value::as_str) != Some("text") {
                return None;
            }
            let text = part.get("text").and_then(Value::as_str)?;
            Some((text, format!("{base_path}/content/0/text")))
        }
        _ => None,
    }
}

struct ReduceRejection {
    reason_code: &'static str,
    detail: String,
    range: Option<ByteRange>,
}

struct ReduceAttempt<'a> {
    recovery_ref: Option<String>,
    raw: &'a [u8],
    content_str: &'a str,
    file_path: &'a str,
    producer: &'a str,
    tool_use_id: &'a str,
    content_json_path: &'a str,
    head_lines: usize,
    tail_lines: usize,
    first_line: u64,
}

struct Replacement {
    text: String,
    reason_code: &'static str,
    detail: String,
}

fn attempt_reduction(
    attempt: &ReduceAttempt<'_>,
) -> Result<(TargetEntry, PendingSplice), ReduceRejection> {
    let reduction = build_reduction(&ReductionInput {
        content: attempt.content_str,
        file_path: attempt.file_path,
        producer: attempt.producer,
        head_lines: attempt.head_lines,
        tail_lines: attempt.tail_lines,
        first_line: attempt.first_line,
    });
    let detail = format!(
        "file {}: kept file lines {}-{} and {}-{} ({} lines omitted); captured-original recovery is recorded when available",
        attempt.file_path,
        reduction.first_line,
        reduction.head_last,
        reduction.tail_first,
        reduction.last_line,
        reduction.omitted_count,
    );
    splice_replacement(
        attempt,
        Replacement {
            text: reduction.text,
            reason_code: reason::FILE_READ_REDUCED,
            detail,
        },
    )
}

fn attempt_supersession(
    attempt: &ReduceAttempt<'_>,
) -> Result<(TargetEntry, PendingSplice), ReduceRejection> {
    let result_lines = attempt.content_str.split('\n').count();
    splice_replacement(
        attempt,
        Replacement {
            text: superseded_text(attempt.file_path, result_lines),
            reason_code: reason::FILE_READ_SUPERSEDED,
            detail: format!(
                "file {}: a later whole read of the same file is in the conversation; this older copy is replaced by a pointer to it",
                attempt.file_path
            ),
        },
    )
}

fn locate(raw: &[u8], needle: &[u8]) -> Result<usize, ReduceRejection> {
    let Some(first_idx) = find_subsequence(raw, needle) else {
        return Err(ReduceRejection {
            reason_code: reason::RANGE_NOT_FOUND,
            detail:
                "the original content's JSON-encoded bytes were not found verbatim in the raw body"
                    .to_string(),
            range: None,
        });
    };
    if find_subsequence(&raw[first_idx + needle.len()..], needle).is_some() {
        return Err(ReduceRejection {
            reason_code: reason::RANGE_AMBIGUOUS,
            detail:
                "the original content's JSON-encoded bytes appear more than once in the raw body"
                    .to_string(),
            range: Some(ByteRange {
                start: first_idx,
                end: first_idx + needle.len(),
            }),
        });
    }
    Ok(first_idx)
}

fn encode(text: &str, what: &str) -> Result<Vec<u8>, ReduceRejection> {
    serde_json::to_vec(text).map_err(|e| ReduceRejection {
        reason_code: reason::ENCODE_FAILED,
        detail: format!("failed to JSON-encode the {what} content: {e}"),
        range: None,
    })
}

fn splice_replacement(
    attempt: &ReduceAttempt<'_>,
    mut replacement: Replacement,
) -> Result<(TargetEntry, PendingSplice), ReduceRejection> {
    if let Some(reference) = &attempt.recovery_ref {
        replacement.text.push_str(&format!("\nExact captured original: call get_request_body with {reference}. This returns the original JSON string, independent of current file contents."));
    }
    if replacement.text.len() >= attempt.content_str.len() {
        return Err(ReduceRejection {
            reason_code: reason::NO_SHRINK,
            detail: format!(
                "reduced form is {} bytes, not shorter than the {} original",
                replacement.text.len(),
                attempt.content_str.len()
            ),
            range: None,
        });
    }
    let original_encoded = encode(attempt.content_str, "original")?;
    let new_encoded = encode(&replacement.text, "reduced")?;
    let first_idx = locate(attempt.raw, &original_encoded)?;
    let input_range = ByteRange {
        start: first_idx,
        end: first_idx + original_encoded.len(),
    };
    let target = TargetEntry {
        outcome: StageOutcome::Applied,
        content_kind: "read_result_text".to_string(),
        json_path: Some(attempt.content_json_path.to_string()),
        tool_use_id: Some(attempt.tool_use_id.to_string()),
        producer: Some(attempt.producer.to_string()),
        input_range,
        output_range: ByteRange::UNLOCATED,
        original_bytes: attempt.content_str.len(),
        sent_bytes: replacement.text.len(),
        original_tokens: Some((attempt.content_str.len() / 4) as u64),
        sent_tokens: Some((replacement.text.len() / 4) as u64),
        original_content_sha256: sha256_hex(attempt.content_str.as_bytes()),
        sent_content_sha256: sha256_hex(replacement.text.as_bytes()),
        recovery_ref: attempt.recovery_ref.clone(),
        reason_code: replacement.reason_code.to_string(),
        detail: Some(replacement.detail),
    };
    let pending = PendingSplice {
        start: input_range.start,
        end: input_range.end,
        new_bytes: new_encoded,
        target_index: 0,
    };
    Ok((target, pending))
}

pub fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::find_subsequence;

    #[test]
    fn find_subsequence_locates_needle_and_returns_none_when_absent() {
        assert_eq!(find_subsequence(b"abcdef", b"cd"), Some(2));
        assert_eq!(find_subsequence(b"abcdef", b"zz"), None);
        assert_eq!(find_subsequence(b"abc", b""), None);
        assert_eq!(find_subsequence(b"ab", b"abc"), None);
    }
}
