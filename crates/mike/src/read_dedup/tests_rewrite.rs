use super::tests_support::ts;
use super::*;
use serde_json::json;

#[test]
fn rewrite_returns_none_when_no_hit() {
    let body = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}"}
            ]}
        ]
    });
    let mut index = SessionDedupIndex::default();
    let mut stats = SessionDedupStats::default();
    let (hits, rewritten) = rewrite(&body, &mut index, &mut stats, ts());
    assert!(hits.is_empty());
    assert!(rewritten.is_none());
}

#[test]
fn rewrite_replaces_duplicate_read_content_with_stub() -> anyhow::Result<()> {
    let mut index = SessionDedupIndex::default();
    let mut stats = SessionDedupStats::default();

    let first = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}"}
            ]}
        ]
    });
    rewrite(&first, &mut index, &mut stats, ts());

    let second = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}"}
            ]},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t2", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t2", "content": "fn main() {}"}
            ]}
        ]
    });
    let (hits, rewritten) = rewrite(&second, &mut index, &mut stats, ts());
    assert_eq!(hits.len(), 1);
    let new_body = rewritten.ok_or_else(|| anyhow::anyhow!("expected a rewritten body"))?;
    let stubbed = new_body
        .pointer("/messages/3/content/0/content")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("expected the stubbed tool_result content"))?;
    assert_eq!(stubbed, stub_text(&hits[0]));
    assert_eq!(
        new_body
            .pointer("/messages/3/content/0/tool_use_id")
            .and_then(Value::as_str),
        Some("t2"),
        "the tool_result block identity must be preserved, only its content replaced"
    );
    Ok(())
}

fn invalidation_blocks_rewrite(tool_name: &str, invalidating_input: Value) {
    let mut index = SessionDedupIndex::default();
    let mut stats = SessionDedupStats::default();

    let read1 = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}"}
            ]}
        ]
    });
    rewrite(&read1, &mut index, &mut stats, ts());
    assert_eq!(index.len(), 1);

    let invalidate = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}"}
            ]},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t2", "name": tool_name, "input": invalidating_input}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t2", "content": "ok"}
            ]}
        ]
    });
    rewrite(&invalidate, &mut index, &mut stats, ts());
    assert!(
        index.is_empty(),
        "{tool_name} on the indexed path must evict it"
    );

    let read2 = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t3", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t3", "content": "fn main() {}"}
            ]}
        ]
    });
    let (hits, rewritten) = rewrite(&read2, &mut index, &mut stats, ts());
    assert!(
        hits.is_empty(),
        "post-{tool_name} re-read of the same content must NOT be counted as a hit"
    );
    assert!(
        rewritten.is_none(),
        "post-{tool_name} re-read must not be rewritten"
    );
}

#[test]
fn rewrite_edit_invalidates_so_next_identical_read_is_not_rewritten() {
    invalidation_blocks_rewrite(
        "Edit",
        json!({"file_path": "/tmp/a.rs", "old_string": "a", "new_string": "b"}),
    );
}

#[test]
fn rewrite_write_invalidates_so_next_identical_read_is_not_rewritten() {
    invalidation_blocks_rewrite(
        "Write",
        json!({"file_path": "/tmp/a.rs", "content": "new content"}),
    );
}

#[test]
fn rewrite_notebook_edit_invalidates_so_next_identical_read_is_not_rewritten() {
    invalidation_blocks_rewrite(
        "NotebookEdit",
        json!({"notebook_path": "/tmp/a.rs", "file_path": "/tmp/a.rs", "new_source": "x"}),
    );
}

#[test]
fn rewrite_bash_command_containing_path_invalidates_it() {
    let mut index = SessionDedupIndex::default();
    let mut stats = SessionDedupStats::default();

    let read1 = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}"}
            ]}
        ]
    });
    rewrite(&read1, &mut index, &mut stats, ts());
    assert_eq!(index.len(), 1);

    let mv = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}"}
            ]},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t2", "name": "Bash", "input": {"command": "mv /tmp/a.rs /tmp/b.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t2", "content": "moved"}
            ]}
        ]
    });
    rewrite(&mv, &mut index, &mut stats, ts());
    assert!(
        index.is_empty(),
        "mv referencing the indexed path must evict it"
    );

    let read2 = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t3", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t3", "content": "fn main() {}"}
            ]}
        ]
    });
    let (hits, rewritten) = rewrite(&read2, &mut index, &mut stats, ts());
    assert!(hits.is_empty());
    assert!(rewritten.is_none());
}
