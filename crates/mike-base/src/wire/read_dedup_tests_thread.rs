use serde_json::{Value, json};

use super::tests_support::ts;
use super::*;

fn read_call(id: &str) -> Value {
    json!({"role": "assistant", "content": [
        {"type": "tool_use", "id": id, "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
    ]})
}

fn read_result(id: &str) -> Value {
    json!({"role": "user", "content": [
        {"type": "tool_result", "tool_use_id": id, "content": "fn main() {}"}
    ]})
}

#[test]
fn continue_body_with_earlier_assistant_hits_like_full_body() {
    let mut full_index = SessionDedupIndex::default();
    let mut full_stats = SessionDedupStats::default();
    let first = json!({"messages": [read_call("t1"), read_result("t1")]});
    observe(
        DedupInput::whole_body(&first),
        &mut full_index,
        &mut full_stats,
        ts(),
    );
    let full = json!({"messages": [read_call("t1"), read_result("t1"), read_call("t2"), read_result("t2")]});
    let full_hits = observe(
        DedupInput::whole_body(&full),
        &mut full_index,
        &mut full_stats,
        ts(),
    );

    let mut index = SessionDedupIndex::default();
    let mut stats = SessionDedupStats::default();
    observe(DedupInput::whole_body(&first), &mut index, &mut stats, ts());
    let continued = json!({"messages": [read_result("t2")], "thread": {"type": "continue"}});
    let earlier = read_call("t2");
    let hits = observe(
        DedupInput {
            body: &continued,
            earlier_assistant: Some(&earlier),
        },
        &mut index,
        &mut stats,
        ts(),
    );

    assert_eq!(full_hits.len(), 1);
    assert_eq!(hits.len(), full_hits.len());
}

#[test]
fn continue_body_without_earlier_assistant_is_a_no_op() {
    let mut index = SessionDedupIndex::default();
    let mut stats = SessionDedupStats::default();
    let continued = json!({"messages": [read_result("t2")]});
    assert!(
        observe(
            DedupInput::whole_body(&continued),
            &mut index,
            &mut stats,
            ts()
        )
        .is_empty()
    );
    assert!(index.is_empty());
}
