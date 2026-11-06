use serde_json::json;

use crate::compress::manifest::{MANIFEST_SCHEMA_VERSION, TransformationManifest};

use super::detect::REDUCTION_MARKER_TAG;
use super::reason;
use super::tests_support::{TestPolicy, big_file_content, raw_in_history};
use super::*;

#[test]
fn reduce_file_reads_no_candidate_body_is_byte_identical() {
    let raw: &[u8] =
        br#"{"model":"m","messages":[{"role":"user","content":[{"type":"text","text":"hi"}]}]}"#;
    let tp = TestPolicy::default();
    let result = reduce_file_reads(raw, &tp.policy());
    assert_eq!(result.candidate.as_ref(), raw);
    assert_eq!(result.stage.outcome, StageOutcome::Unchanged);
    assert_eq!(result.stage.reason_code, reason::STAGE_NO_CANDIDATES);
    assert!(result.stage.targets.is_empty());
}

#[test]
fn reduce_file_reads_malformed_json_passes_through_unchanged() {
    let raw: &[u8] = b"not { valid json";
    let tp = TestPolicy::default();
    let result = reduce_file_reads(raw, &tp.policy());
    assert_eq!(result.candidate.as_ref(), raw);
    assert_eq!(result.stage.outcome, StageOutcome::Skipped);
    assert_eq!(result.stage.reason_code, reason::STAGE_MALFORMED_BODY);
}

#[test]
fn reduce_file_reads_top_level_array_is_malformed() {
    let raw: &[u8] = b"[1,2,3]";
    let tp = TestPolicy::default();
    let result = reduce_file_reads(raw, &tp.policy());
    assert_eq!(result.candidate.as_ref(), raw);
    assert_eq!(result.stage.reason_code, reason::STAGE_MALFORMED_BODY);
}

#[test]
fn reduce_file_reads_applies_and_preserves_surrounding_bytes() -> anyhow::Result<()> {
    let content = big_file_content(200);
    let body = json!({
        "model": "m",
        "system": [{"type": "text", "text": "SENTINEL-PREFIX-UNCHANGED"}],
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/big.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": content}
            ]},
            {"role": "user", "content": [
                {"type": "text", "text": "SENTINEL-SUFFIX-UNCHANGED"}
            ]}
        ]
    });
    let raw = raw_in_history(body)?;
    let tp = TestPolicy::default();
    let result = reduce_file_reads(&raw, &tp.policy());

    assert_eq!(result.stage.outcome, StageOutcome::Applied);
    assert_eq!(result.stage.targets.len(), 1);
    let target = &result.stage.targets[0];
    assert_eq!(target.outcome, StageOutcome::Applied);
    assert_eq!(target.reason_code, reason::FILE_READ_REDUCED);
    assert_eq!(target.producer.as_deref(), Some("Read"));
    assert!(result.candidate.len() < raw.len());

    let candidate_str = String::from_utf8(result.candidate.to_vec())?;
    assert!(candidate_str.contains("SENTINEL-PREFIX-UNCHANGED"));
    assert!(candidate_str.contains("SENTINEL-SUFFIX-UNCHANGED"));
    assert!(candidate_str.contains(REDUCTION_MARKER_TAG));

    let raw_before = &raw[..target.input_range.start];
    let raw_after = &raw[target.input_range.end..];
    let candidate_before = &result.candidate[..target.output_range.start];
    let candidate_after = &result.candidate[target.output_range.end..];
    assert_eq!(raw_before, candidate_before);
    assert_eq!(raw_after, candidate_after);
    Ok(())
}

#[test]
fn reduce_file_reads_preserves_signed_thinking_block_byte_identical() -> anyhow::Result<()> {
    let content = big_file_content(200);
    let body = json!({
        "model": "m",
        "messages": [
            {"role": "assistant", "content": [
                {"type": "thinking", "thinking": "some private reasoning text", "signature": "SIGNATURE-BYTES-ABC123=="}
            ]},
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

    let raw_str = String::from_utf8(raw.clone())?;
    let candidate_str = String::from_utf8(result.candidate.to_vec())?;
    let signature_needle = "\"signature\":\"SIGNATURE-BYTES-ABC123==\"";
    let thinking_needle = "\"thinking\":\"some private reasoning text\"";
    assert!(raw_str.contains(signature_needle));
    assert!(candidate_str.contains(signature_needle));
    assert!(raw_str.contains(thinking_needle));
    assert!(candidate_str.contains(thinking_needle));
    Ok(())
}

#[test]
fn reduce_file_reads_is_deterministic_and_idempotent() -> anyhow::Result<()> {
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

    let first = reduce_file_reads(&raw, &tp.policy());
    let second = reduce_file_reads(&raw, &tp.policy());
    assert_eq!(first.candidate, second.candidate);
    assert_eq!(first.stage, second.stage);

    let third = reduce_file_reads(&first.candidate, &tp.policy());
    assert_eq!(third.candidate.as_ref(), first.candidate.as_ref());
    assert_eq!(third.stage.outcome, StageOutcome::Unchanged);
    assert_eq!(third.stage.targets.len(), 1);
    assert_eq!(third.stage.targets[0].reason_code, reason::ALREADY_REDUCED);
    Ok(())
}

#[test]
fn reduce_file_reads_no_shrink_fallback_leaves_bytes_unchanged() -> anyhow::Result<()> {
    let content = format!("{}\ny\nz", "x".repeat(DEFAULT_MIN_RESULT_BYTES + 50));
    let body = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/one-line.txt"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": content}
            ]}
        ]
    });
    let raw = raw_in_history(body)?;
    let tp = TestPolicy::default();
    let result = reduce_file_reads(&raw, &tp.policy());
    assert_eq!(result.candidate.as_ref(), raw.as_slice());
    assert_eq!(result.stage.outcome, StageOutcome::FailedClosed);
    assert_eq!(result.stage.reason_code, reason::STAGE_ALL_FAILED_CLOSED);
    assert_eq!(result.stage.targets.len(), 1);
    assert_eq!(result.stage.targets[0].reason_code, reason::NO_SHRINK);
    Ok(())
}

#[test]
fn reduce_file_reads_unpaired_result_passes_through_unchanged() -> anyhow::Result<()> {
    let content = big_file_content(200);
    let body = json!({
        "messages": [
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "no-such-id", "content": content}
            ]}
        ]
    });
    let raw = raw_in_history(body)?;
    let tp = TestPolicy::default();
    let result = reduce_file_reads(&raw, &tp.policy());
    assert_eq!(result.candidate.as_ref(), raw.as_slice());
    assert_eq!(result.stage.targets.len(), 1);
    assert_eq!(result.stage.targets[0].reason_code, reason::UNPAIRED_RESULT);
    assert_eq!(result.stage.targets[0].outcome, StageOutcome::Skipped);
    assert_eq!(result.stage.reason_code, reason::STAGE_NO_CANDIDATES);
    Ok(())
}

#[test]
fn reduce_file_reads_small_read_passes_through_unchanged() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/tiny.txt"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "just a few bytes"}
            ]}
        ]
    });
    let raw = raw_in_history(body)?;
    let tp = TestPolicy::default();
    let result = reduce_file_reads(&raw, &tp.policy());
    assert_eq!(result.candidate.as_ref(), raw.as_slice());
    assert_eq!(result.stage.targets[0].reason_code, reason::SMALL_READ);
    Ok(())
}

#[test]
fn reduce_file_reads_error_result_passes_through_unchanged() -> anyhow::Result<()> {
    let content = big_file_content(200);
    let body = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/big.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": content, "is_error": true}
            ]}
        ]
    });
    let raw = raw_in_history(body)?;
    let tp = TestPolicy::default();
    let result = reduce_file_reads(&raw, &tp.policy());
    assert_eq!(result.candidate.as_ref(), raw.as_slice());
    assert_eq!(result.stage.targets[0].reason_code, reason::ERROR_RESULT);
    Ok(())
}

#[test]
fn reduce_file_reads_non_string_content_shape_passes_through_unchanged() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/multi.txt"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": [
                    {"type": "text", "text": "part one"},
                    {"type": "text", "text": "part two"}
                ]}
            ]}
        ]
    });
    let raw = raw_in_history(body)?;
    let tp = TestPolicy::default();
    let result = reduce_file_reads(&raw, &tp.policy());
    assert_eq!(result.candidate.as_ref(), raw.as_slice());
    assert_eq!(
        result.stage.targets[0].reason_code,
        reason::NON_STRING_CONTENT
    );
    Ok(())
}

#[test]
fn reduce_file_reads_ambiguous_needle_fails_closed_for_both_occurrences() -> anyhow::Result<()> {
    let content = big_file_content(200);
    let body = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/big.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": content}
            ]},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t2", "name": "Read", "input": {"file_path": "/tmp/big.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t2", "content": content}
            ]}
        ]
    });
    let raw = raw_in_history(body)?;
    let tp = TestPolicy::default();
    let result = reduce_file_reads(&raw, &tp.policy());
    assert_eq!(result.candidate.as_ref(), raw.as_slice());
    assert_eq!(result.stage.outcome, StageOutcome::FailedClosed);
    assert_eq!(result.stage.targets.len(), 2);
    assert!(
        result
            .stage
            .targets
            .iter()
            .all(|t| t.reason_code == reason::RANGE_AMBIGUOUS)
    );
    Ok(())
}

#[test]
fn reduce_file_reads_pairs_multiple_reads_by_tool_use_id_not_position() -> anyhow::Result<()> {
    let content_a = big_file_content(200).replace("line 1 ", "AAA-FILE line 1 ");
    let content_b = big_file_content(210).replace("line 1 ", "BBB-FILE line 1 ");
    let body = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t2", "name": "Read", "input": {"file_path": "/tmp/b.rs"}},
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": content_a},
                {"type": "tool_result", "tool_use_id": "t2", "content": content_b}
            ]}
        ]
    });
    let raw = raw_in_history(body)?;
    let tp = TestPolicy::default();
    let result = reduce_file_reads(&raw, &tp.policy());
    assert_eq!(result.stage.outcome, StageOutcome::Applied);
    assert_eq!(result.stage.targets.len(), 2);
    assert!(
        result
            .stage
            .targets
            .iter()
            .all(|t| t.outcome == StageOutcome::Applied)
    );

    let target_t1 = result
        .stage
        .targets
        .iter()
        .find(|t| t.tool_use_id.as_deref() == Some("t1"))
        .ok_or_else(|| anyhow::anyhow!("t1 target missing"))?;
    let target_t2 = result
        .stage
        .targets
        .iter()
        .find(|t| t.tool_use_id.as_deref() == Some("t2"))
        .ok_or_else(|| anyhow::anyhow!("t2 target missing"))?;

    let region_t1 = std::str::from_utf8(
        &result.candidate[target_t1.output_range.start..target_t1.output_range.end],
    )?;
    let region_t2 = std::str::from_utf8(
        &result.candidate[target_t2.output_range.start..target_t2.output_range.end],
    )?;
    assert!(region_t1.contains("file=/tmp/a.rs"));
    assert!(!region_t1.contains("file=/tmp/b.rs"));
    assert!(region_t2.contains("file=/tmp/b.rs"));
    assert!(!region_t2.contains("file=/tmp/a.rs"));
    Ok(())
}

#[test]
fn reduce_file_reads_resolved_parameters_carries_policy_values() {
    let tp = TestPolicy::default().with_protected(&["**/CLAUDE.md"]);
    let raw: &[u8] = br#"{"messages":[]}"#;
    let result = reduce_file_reads(raw, &tp.policy());
    let params = result.stage.resolved_parameters;
    assert_eq!(params["min_result_bytes"], DEFAULT_MIN_RESULT_BYTES);
    assert_eq!(params["head_lines"], DEFAULT_HEAD_LINES);
    assert_eq!(params["tail_lines"], DEFAULT_TAIL_LINES);
    assert_eq!(params["grace_turns"], 0);
    assert_eq!(params["enabled_producers"], json!(["Read"]));
    assert_eq!(params["protected_paths"], json!(["**/CLAUDE.md"]));
}

#[test]
fn reduce_file_reads_manifest_chain_closes_on_applied_and_no_op() -> anyhow::Result<()> {
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
    let applied = reduce_file_reads(&raw, &tp.policy());
    let manifest_applied = TransformationManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        original_sha256: sha256_hex(&raw),
        sent_sha256: sha256_hex(&applied.candidate),
        stages: vec![applied.stage],
    };
    assert!(manifest_applied.verify_chain().is_ok());

    let no_op_raw: &[u8] =
        br#"{"messages":[{"role":"user","content":[{"type":"text","text":"hi"}]}]}"#;
    let no_op = reduce_file_reads(no_op_raw, &tp.policy());
    let manifest_no_op = TransformationManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        original_sha256: sha256_hex(no_op_raw),
        sent_sha256: sha256_hex(&no_op.candidate),
        stages: vec![no_op.stage],
    };
    assert!(manifest_no_op.verify_chain().is_ok());
    Ok(())
}
