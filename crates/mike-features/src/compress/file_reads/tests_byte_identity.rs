use serde_json::json;

use super::tests_support::{TestPolicy, big_file_content, raw_in_history};
use super::*;

#[test]
fn reduce_file_reads_large_system_prompt_block_is_byte_identical() -> anyhow::Result<()> {
    let huge = big_file_content(500);
    let body = json!({
        "model": "m",
        "system": [{"type": "text", "text": huge}],
        "messages": [{"role": "user", "content": [{"type": "text", "text": "hi"}]}]
    });
    let raw = raw_in_history(body)?;
    let tp = TestPolicy::default();
    let result = reduce_file_reads(&raw, &tp.policy());
    assert_eq!(result.candidate.as_ref(), raw.as_slice());
    assert_eq!(result.stage.outcome, StageOutcome::Unchanged);
    assert!(result.stage.targets.is_empty());
    Ok(())
}

#[test]
fn reduce_file_reads_large_operator_text_block_is_byte_identical() -> anyhow::Result<()> {
    let huge = big_file_content(500);
    let body = json!({
        "messages": [{"role": "user", "content": [{"type": "text", "text": huge}]}]
    });
    let raw = raw_in_history(body)?;
    let tp = TestPolicy::default();
    let result = reduce_file_reads(&raw, &tp.policy());
    assert_eq!(result.candidate.as_ref(), raw.as_slice());
    assert_eq!(result.stage.outcome, StageOutcome::Unchanged);
    assert!(result.stage.targets.is_empty());
    Ok(())
}

#[test]
fn reduce_file_reads_large_assistant_response_is_byte_identical() -> anyhow::Result<()> {
    let huge = big_file_content(500);
    let body = json!({
        "messages": [{"role": "assistant", "content": [{"type": "text", "text": huge}]}]
    });
    let raw = raw_in_history(body)?;
    let tp = TestPolicy::default();
    let result = reduce_file_reads(&raw, &tp.policy());
    assert_eq!(result.candidate.as_ref(), raw.as_slice());
    assert_eq!(result.stage.outcome, StageOutcome::Unchanged);
    assert!(result.stage.targets.is_empty());
    Ok(())
}

#[test]
fn reduce_file_reads_large_non_read_tool_result_is_byte_identical() -> anyhow::Result<()> {
    let huge = big_file_content(500);
    let body = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "cat big.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": huge}
            ]}
        ]
    });
    let raw = raw_in_history(body)?;
    let tp = TestPolicy::default();
    let result = reduce_file_reads(&raw, &tp.policy());
    assert_eq!(result.candidate.as_ref(), raw.as_slice());
    assert_eq!(result.stage.outcome, StageOutcome::Unchanged);
    assert!(result.stage.targets.is_empty());
    Ok(())
}

#[test]
fn reduce_file_reads_protected_path_is_never_reduced_regardless_of_size() -> anyhow::Result<()> {
    let content = big_file_content(500);
    let body = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/repo/CLAUDE.md"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": content}
            ]}
        ]
    });
    let raw = raw_in_history(body)?;
    let tp = TestPolicy::default().with_protected(&["**/CLAUDE.md"]);
    let result = reduce_file_reads(&raw, &tp.policy());
    assert_eq!(result.candidate.as_ref(), raw.as_slice());
    assert_eq!(result.stage.targets.len(), 1);
    assert_eq!(result.stage.targets[0].outcome, StageOutcome::Skipped);
    assert_eq!(result.stage.targets[0].reason_code, reason::PROTECTED_PATH);
    Ok(())
}

#[test]
fn reduce_file_reads_non_protected_path_reduces_normally() -> anyhow::Result<()> {
    let content = big_file_content(200);
    let body = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/ordinary.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": content}
            ]}
        ]
    });
    let raw = raw_in_history(body)?;
    let tp = TestPolicy::default().with_protected(&["**/CLAUDE.md"]);
    let result = reduce_file_reads(&raw, &tp.policy());
    assert_eq!(result.stage.outcome, StageOutcome::Applied);
    Ok(())
}

#[test]
fn reduce_file_reads_empty_protected_paths_protects_nothing() -> anyhow::Result<()> {
    let content = big_file_content(500);
    let body = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/repo/CLAUDE.md"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": content}
            ]}
        ]
    });
    let raw = raw_in_history(body)?;
    let tp = TestPolicy::default();
    assert!(tp.protected_paths.is_empty());
    let result = reduce_file_reads(&raw, &tp.policy());
    assert_eq!(result.stage.outcome, StageOutcome::Applied);
    Ok(())
}

#[test]
fn reduce_file_reads_enabled_producer_generalizes_beyond_read() -> anyhow::Result<()> {
    let content = big_file_content(500);
    let body = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Grep", "input": {"file_path": "/tmp/grep-out.txt"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": content}
            ]}
        ]
    });
    let raw = raw_in_history(body)?;
    let tp = TestPolicy::default().with_producers(&["Grep"]);
    let result = reduce_file_reads(&raw, &tp.policy());
    assert_eq!(result.stage.outcome, StageOutcome::Applied);
    assert_eq!(result.stage.targets.len(), 1);
    assert_eq!(result.stage.targets[0].producer.as_deref(), Some("Grep"));

    let candidate_str = String::from_utf8(result.candidate.to_vec())?;
    assert!(candidate_str.contains("call Grep with"));
    Ok(())
}

#[test]
fn reduce_file_reads_empty_enabled_producers_disables_even_read() -> anyhow::Result<()> {
    let content = big_file_content(500);
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
    let tp = TestPolicy::default().with_producers(&[]);
    let result = reduce_file_reads(&raw, &tp.policy());
    assert_eq!(result.candidate.as_ref(), raw.as_slice());
    assert_eq!(result.stage.outcome, StageOutcome::Unchanged);
    assert!(result.stage.targets.is_empty());
    Ok(())
}
