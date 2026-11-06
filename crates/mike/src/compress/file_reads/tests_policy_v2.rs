use serde_json::{Value, json};

use super::reason;
use super::supersede::SUPERSEDED_MARKER_TAG;
use super::tests_support::{TestPolicy, big_file_content};
use super::*;

fn read(id: &str, path: &str, input_extra: Value, content: &str) -> [Value; 2] {
    let mut input = json!({"file_path": path});
    if let (Some(target), Some(extra)) = (input.as_object_mut(), input_extra.as_object()) {
        target.extend(extra.clone());
    }
    [
        json!({"role": "assistant", "content": [
            {"type": "tool_use", "id": id, "name": "Read", "input": input}
        ]}),
        json!({"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": id, "content": content}
        ]}),
    ]
}

fn edit(id: &str, path: &str) -> [Value; 2] {
    [
        json!({"role": "assistant", "content": [
            {"type": "tool_use", "id": id, "name": "Edit",
             "input": {"file_path": path, "old_string": "a", "new_string": "b"}}
        ]}),
        json!({"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": id, "content": "edited"}
        ]}),
    ]
}

fn closing_turn() -> [Value; 2] {
    [
        json!({"role": "assistant", "content": [{"type": "text", "text": "noted"}]}),
        json!({"role": "user", "content": [{"type": "text", "text": "continue"}]}),
    ]
}

fn body(parts: Vec<[Value; 2]>) -> anyhow::Result<Vec<u8>> {
    let messages: Vec<Value> = parts.into_iter().flatten().collect();
    Ok(serde_json::to_vec(&json!({"messages": messages}))?)
}

fn target<'a>(reduced: &'a FileReadReduction, id: &str) -> anyhow::Result<&'a TargetEntry> {
    reduced
        .stage
        .targets
        .iter()
        .find(|t| t.tool_use_id.as_deref() == Some(id))
        .ok_or_else(|| anyhow::anyhow!("no target for {id}"))
}

#[test]
fn an_older_whole_read_is_replaced_by_a_pointer_when_a_newer_whole_read_follows()
-> anyhow::Result<()> {
    let content = big_file_content(200);
    let raw = body(vec![
        read("t1", "/tmp/a.rs", json!({}), &content),
        read("t2", "/tmp/a.rs", json!({}), &format!("{content}changed\n")),
        closing_turn(),
    ])?;
    let policy = TestPolicy::default().with_grace(3);
    let reduced = reduce_file_reads(&raw, &policy.policy());
    let older = target(&reduced, "t1")?;
    assert_eq!(older.outcome, StageOutcome::Applied);
    assert_eq!(older.reason_code, reason::FILE_READ_SUPERSEDED);
    let newer = target(&reduced, "t2")?;
    assert_eq!(newer.reason_code, reason::GRACE_WINDOW);
    let text = String::from_utf8(reduced.candidate.to_vec())?;
    assert!(text.contains(SUPERSEDED_MARKER_TAG));
    Ok(())
}

#[test]
fn a_partial_newer_read_does_not_supersede_a_whole_one() -> anyhow::Result<()> {
    let content = big_file_content(200);
    let raw = body(vec![
        read("t1", "/tmp/a.rs", json!({}), &content),
        read(
            "t2",
            "/tmp/a.rs",
            json!({"offset": 10, "limit": 5}),
            "short",
        ),
        closing_turn(),
    ])?;
    let policy = TestPolicy::default().with_grace(3);
    let reduced = reduce_file_reads(&raw, &policy.policy());
    assert_eq!(target(&reduced, "t1")?.reason_code, reason::GRACE_WINDOW);
    Ok(())
}

#[test]
fn a_read_of_a_file_edited_since_is_shortened_inside_the_grace_window() -> anyhow::Result<()> {
    let raw = body(vec![
        read("t1", "/tmp/a.md", json!({}), &big_file_content(200)),
        edit("e1", "/tmp/a.md"),
        closing_turn(),
    ])?;
    let policy = TestPolicy::default().with_grace(5);
    let reduced = reduce_file_reads(&raw, &policy.policy());
    let older = target(&reduced, "t1")?;
    assert_eq!(older.outcome, StageOutcome::Applied);
    assert_eq!(older.reason_code, reason::FILE_READ_REDUCED);
    Ok(())
}

#[test]
fn a_read_inside_the_grace_window_stays_whole() -> anyhow::Result<()> {
    let raw = body(vec![
        read("t1", "/tmp/a.md", json!({}), &big_file_content(200)),
        closing_turn(),
        closing_turn(),
    ])?;
    let policy = TestPolicy::default().with_grace(3);
    let reduced = reduce_file_reads(&raw, &policy.policy());
    assert_eq!(reduced.candidate.as_ref(), raw.as_slice());
    assert_eq!(target(&reduced, "t1")?.reason_code, reason::GRACE_WINDOW);
    Ok(())
}

#[test]
fn a_protected_path_is_never_superseded() -> anyhow::Result<()> {
    let content = big_file_content(200);
    let raw = body(vec![
        read("t1", "/repo/CLAUDE.md", json!({}), &content),
        read("t2", "/repo/CLAUDE.md", json!({}), &content),
        closing_turn(),
    ])?;
    let policy = TestPolicy::default()
        .with_grace(3)
        .with_protected(&["**/CLAUDE.md"]);
    let reduced = reduce_file_reads(&raw, &policy.policy());
    assert_eq!(reduced.candidate.as_ref(), raw.as_slice());
    assert_eq!(target(&reduced, "t1")?.reason_code, reason::PROTECTED_PATH);
    Ok(())
}

#[test]
fn a_warm_cache_holds_new_cuts_and_keeps_earlier_ones() -> anyhow::Result<()> {
    let raw = body(vec![
        read("t1", "/tmp/a.md", json!({}), &big_file_content(200)),
        read("t2", "/tmp/b.md", json!({}), &big_file_content(210)),
        closing_turn(),
    ])?;
    let policy = TestPolicy::default();
    let earlier: std::collections::HashSet<String> = ["t1".to_string()].into();
    let reduced = reduce_file_reads(&raw, &policy.policy().with_warm_cuts(Some(&earlier)));
    assert_eq!(target(&reduced, "t1")?.outcome, StageOutcome::Applied);
    let held = target(&reduced, "t2")?;
    assert_eq!(held.outcome, StageOutcome::Skipped);
    assert_eq!(held.reason_code, reason::CACHE_WARM);
    Ok(())
}

#[test]
fn a_cold_cache_allows_every_cut() -> anyhow::Result<()> {
    let raw = body(vec![
        read("t1", "/tmp/a.md", json!({}), &big_file_content(200)),
        read("t2", "/tmp/b.md", json!({}), &big_file_content(210)),
        closing_turn(),
    ])?;
    let policy = TestPolicy::default();
    let reduced = reduce_file_reads(&raw, &policy.policy().with_warm_cuts(None));
    assert_eq!(target(&reduced, "t1")?.outcome, StageOutcome::Applied);
    assert_eq!(target(&reduced, "t2")?.outcome, StageOutcome::Applied);
    Ok(())
}

#[test]
fn the_pointer_is_not_reduced_again_on_the_next_request() -> anyhow::Result<()> {
    let content = big_file_content(200);
    let raw = body(vec![
        read("t1", "/tmp/a.rs", json!({}), &content),
        read("t2", "/tmp/a.rs", json!({}), &format!("{content}changed\n")),
        closing_turn(),
    ])?;
    let policy = TestPolicy::default().with_grace(3);
    let first = reduce_file_reads(&raw, &policy.policy());
    let second = reduce_file_reads(&first.candidate, &policy.policy());
    assert_eq!(second.candidate, first.candidate);
    Ok(())
}
