use std::collections::{HashMap, HashSet};

use serde_json::Value;
use tracing::warn;

use crate::wire::manifest::{ByteRange, StageOutcome, TargetEntry, sha256_hex};

use super::STAGE_NAME;
use super::detect::extract_content;
use super::reason;

pub struct PendingSplice {
    pub start: usize,
    pub end: usize,
    pub new_bytes: Vec<u8>,
    pub target_index: usize,
}

pub fn resolve_overlaps(
    targets: &mut [TargetEntry],
    pending: &mut Vec<PendingSplice>,
) {
    pending.sort_by_key(|p| p.start);
    let mut accepted: Vec<PendingSplice> = Vec::with_capacity(pending.len());
    let mut last_end: Option<usize> = None;
    for splice in pending.drain(..) {
        if last_end.is_some_and(|le| splice.start < le) {
            let t = &mut targets[splice.target_index];
            t.outcome = StageOutcome::FailedClosed;
            t.reason_code = reason::RANGE_OVERLAP.to_string();
            t.detail = Some(
                "this target's located byte range overlaps another accepted replacement"
                    .to_string(),
            );
            t.output_range = t.input_range;
            t.sent_bytes = t.original_bytes;
            t.sent_tokens = t.original_tokens;
            t.sent_content_sha256 = t.original_content_sha256.clone();
            continue;
        }
        last_end = Some(splice.end);
        accepted.push(splice);
    }
    *pending = accepted;
}

fn read_tool_result_content_paths(body: &Value, enabled_producers: &[String]) -> HashSet<String> {
    let mut allowed = HashSet::new();
    let Some(messages) = body.get("messages").and_then(Value::as_array) else {
        return allowed;
    };
    let mut use_by_id: HashMap<&str, &str> = HashMap::new();
    for message in messages {
        let Some(blocks) = message.get("content").and_then(Value::as_array) else {
            continue;
        };
        for block in blocks {
            if block.get("type").and_then(Value::as_str) != Some("tool_use") {
                continue;
            }
            if let (Some(id), Some(name)) = (
                block.get("id").and_then(Value::as_str),
                block.get("name").and_then(Value::as_str),
            ) {
                use_by_id.insert(id, name);
            }
        }
    }
    for (msg_idx, message) in messages.iter().enumerate() {
        let Some(blocks) = message.get("content").and_then(Value::as_array) else {
            continue;
        };
        for (block_idx, block) in blocks.iter().enumerate() {
            if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                continue;
            }
            let Some(tool_use_id) = block.get("tool_use_id").and_then(Value::as_str) else {
                continue;
            };
            let Some(&name) = use_by_id.get(tool_use_id) else {
                continue;
            };
            if !enabled_producers.iter().any(|p| p.as_str() == name) {
                continue;
            }
            let base_path = format!("/messages/{msg_idx}/content/{block_idx}");
            if let Some((_, content_json_path)) = extract_content(block, &base_path) {
                allowed.insert(content_json_path);
            }
        }
    }
    allowed
}

fn splice_is_verified(raw: &[u8], allowed_paths: &HashSet<String>, target: &TargetEntry) -> bool {
    let Some(json_path) = target.json_path.as_deref() else {
        return false;
    };
    if !allowed_paths.contains(json_path) {
        return false;
    }
    let range = target.input_range;
    let Some(range_bytes) = raw.get(range.start..range.end) else {
        return false;
    };
    let Ok(range_str) = std::str::from_utf8(range_bytes) else {
        return false;
    };
    let Ok(decoded) = serde_json::from_str::<String>(range_str) else {
        return false;
    };
    sha256_hex(decoded.as_bytes()) == target.original_content_sha256
}

pub fn enforce_read_tool_result_boundary(
    raw: &[u8],
    body: &Value,
    enabled_producers: &[String],
    targets: &mut [TargetEntry],
    pending: &mut Vec<PendingSplice>,
) {
    let allowed_paths = read_tool_result_content_paths(body, enabled_producers);
    let mut verified: Vec<PendingSplice> = Vec::with_capacity(pending.len());
    for splice in pending.drain(..) {
        let target = &mut targets[splice.target_index];
        if splice_is_verified(raw, &allowed_paths, target) {
            verified.push(splice);
            continue;
        }
        warn!(
            stage = STAGE_NAME,
            tool_use_id = target.tool_use_id.as_deref().unwrap_or(""),
            json_path = target.json_path.as_deref().unwrap_or(""),
            "file-read compression runtime guard violated: splice failed independent re-verification against parsed tool_result content; forwarding untouched"
        );
        target.outcome = StageOutcome::FailedClosed;
        target.reason_code = reason::RUNTIME_GUARD_VIOLATION.to_string();
        target.detail = Some(
            "the located splice range failed independent re-verification against the parsed body's enabled-producer tool_result content fields"
                .to_string(),
        );
        target.output_range = target.input_range;
        target.sent_bytes = target.original_bytes;
        target.sent_tokens = target.original_tokens;
        target.sent_content_sha256 = target.original_content_sha256.clone();
    }
    *pending = verified;
}

pub fn build_candidate(
    raw: &[u8],
    accepted: &[PendingSplice],
    targets: &mut [TargetEntry],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    let mut cursor = 0usize;
    for splice in accepted {
        out.extend_from_slice(&raw[cursor..splice.start]);
        let output_start = out.len();
        out.extend_from_slice(&splice.new_bytes);
        let output_end = out.len();
        cursor = splice.end;
        targets[splice.target_index].output_range = ByteRange {
            start: output_start,
            end: output_end,
        };
    }
    out.extend_from_slice(&raw[cursor..]);
    out
}

#[cfg(test)]
pub mod tests {
    use serde_json::json;

    use crate::compress::file_reads::detect::find_subsequence;
    use crate::compress::file_reads::reduce_file_reads;
    use crate::compress::file_reads::tests_support::{
        TestPolicy, big_file_content, raw_in_history,
    };

    use super::*;

    fn applied_stub_target(start: usize, end: usize) -> TargetEntry {
        TargetEntry {
            outcome: StageOutcome::Applied,
            content_kind: "read_result_text".to_string(),
            json_path: None,
            tool_use_id: None,
            producer: Some("Read".to_string()),
            input_range: ByteRange { start, end },
            output_range: ByteRange::UNLOCATED,
            original_bytes: end - start,
            sent_bytes: 10,
            original_tokens: None,
            sent_tokens: None,
            original_content_sha256: sha256_hex(b"x"),
            sent_content_sha256: sha256_hex(b"y"),
            recovery_ref: None,
            reason_code: reason::FILE_READ_REDUCED.to_string(),
            detail: None,
        }
    }

    #[test]
    fn resolve_overlaps_downgrades_the_later_overlapping_target_to_failed_closed() {
        let mut targets = vec![applied_stub_target(0, 100), applied_stub_target(50, 150)];
        let mut pending = vec![
            PendingSplice {
                start: 0,
                end: 100,
                new_bytes: vec![b'a'; 10],
                target_index: 0,
            },
            PendingSplice {
                start: 50,
                end: 150,
                new_bytes: vec![b'b'; 10],
                target_index: 1,
            },
        ];
        resolve_overlaps(&mut targets, &mut pending);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].target_index, 0);
        assert_eq!(targets[0].outcome, StageOutcome::Applied);
        assert_eq!(targets[1].outcome, StageOutcome::FailedClosed);
        assert_eq!(targets[1].reason_code, reason::RANGE_OVERLAP);
        assert_eq!(targets[1].output_range, targets[1].input_range);
    }

    #[test]
    fn enforce_read_tool_result_boundary_fails_closed_on_a_range_outside_any_read_result()
    -> anyhow::Result<()> {
        let body = json!({
            "system": [{"type": "text", "text": "SYSTEM-PROMPT-NEVER-A-SPLICE-TARGET"}],
            "messages": []
        });
        let raw = serde_json::to_vec(&body)?;
        let needle = b"SYSTEM-PROMPT-NEVER-A-SPLICE-TARGET";
        let start =
            find_subsequence(&raw, needle).ok_or_else(|| anyhow::anyhow!("needle not found"))?;
        let end = start + needle.len();

        let mut targets = vec![TargetEntry {
            outcome: StageOutcome::Applied,
            content_kind: "read_result_text".to_string(),
            json_path: Some("/system/0/text".to_string()),
            tool_use_id: Some("bogus".to_string()),
            producer: Some("Read".to_string()),
            input_range: ByteRange { start, end },
            output_range: ByteRange::UNLOCATED,
            original_bytes: needle.len(),
            sent_bytes: 10,
            original_tokens: None,
            sent_tokens: None,
            original_content_sha256: sha256_hex(needle),
            sent_content_sha256: sha256_hex(b"shrunk"),
            recovery_ref: None,
            reason_code: reason::FILE_READ_REDUCED.to_string(),
            detail: None,
        }];
        let mut pending = vec![PendingSplice {
            start,
            end,
            new_bytes: b"shrunk".to_vec(),
            target_index: 0,
        }];

        let parsed: Value = serde_json::from_slice(&raw)?;
        let tp = TestPolicy::default();
        enforce_read_tool_result_boundary(
            &raw,
            &parsed,
            &tp.enabled_producers,
            &mut targets,
            &mut pending,
        );

        assert!(pending.is_empty());
        assert_eq!(targets[0].outcome, StageOutcome::FailedClosed);
        assert_eq!(targets[0].reason_code, reason::RUNTIME_GUARD_VIOLATION);
        assert_eq!(targets[0].output_range, targets[0].input_range);
        assert_eq!(targets[0].sent_bytes, targets[0].original_bytes);
        assert_eq!(
            targets[0].sent_content_sha256,
            targets[0].original_content_sha256
        );
        Ok(())
    }

    #[test]
    fn enforce_read_tool_result_boundary_fails_closed_on_hash_mismatch_at_a_legit_path()
    -> anyhow::Result<()> {
        let content = big_file_content(200);
        let body = json!({
            "messages": [
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/big.rs"}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": content}
                ]}
            ]
        });
        let raw = raw_in_history(body)?;
        let tp = TestPolicy::default();
        let result = reduce_file_reads(&raw, &tp.policy());
        assert_eq!(result.stage.outcome, StageOutcome::Applied);

        let mut corrupted = result.stage.targets[0].clone();
        corrupted.original_content_sha256 = sha256_hex(b"not-the-real-content");
        let mut targets = vec![corrupted];
        let mut pending = vec![PendingSplice {
            start: targets[0].input_range.start,
            end: targets[0].input_range.end,
            new_bytes: b"irrelevant".to_vec(),
            target_index: 0,
        }];

        let parsed: Value = serde_json::from_slice(&raw)?;
        enforce_read_tool_result_boundary(
            &raw,
            &parsed,
            &tp.enabled_producers,
            &mut targets,
            &mut pending,
        );

        assert!(pending.is_empty());
        assert_eq!(targets[0].reason_code, reason::RUNTIME_GUARD_VIOLATION);
        Ok(())
    }
}
