use serde_json::{Value, json};

use super::detect::REDUCTION_MARKER_TAG;
use super::reason;
use super::tests_support::{TestPolicy, big_file_content, numbered_lines, raw_in_history};
use super::*;

fn read_pair(id: &str, path: &str, extra_input: Value, content: &str) -> [Value; 2] {
    let mut input = json!({"file_path": path});
    if let (Some(target), Some(extra)) = (input.as_object_mut(), extra_input.as_object()) {
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

fn marker_line(candidate: &[u8]) -> anyhow::Result<String> {
    let text = String::from_utf8(candidate.to_vec())?;
    let body: Value = serde_json::from_str(&text)?;
    let results = body["messages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("messages missing"))?;
    results
        .iter()
        .filter_map(|m| m["content"][0]["content"].as_str())
        .flat_map(str::lines)
        .find(|line| line.contains(REDUCTION_MARKER_TAG))
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("no marker in candidate"))
}

#[test]
fn freshest_result_in_the_newest_message_is_forwarded_whole() -> anyhow::Result<()> {
    let [call, result] = read_pair("t1", "/tmp/big.rs", json!({}), &big_file_content(200));
    let raw = serde_json::to_vec(&json!({"messages": [call, result]}))?;
    let tp = TestPolicy::default();
    let reduced = reduce_file_reads(&raw, &tp.policy());
    assert_eq!(reduced.candidate.as_ref(), raw.as_slice());
    assert_eq!(reduced.stage.targets.len(), 1);
    assert_eq!(reduced.stage.targets[0].outcome, StageOutcome::Skipped);
    assert_eq!(
        reduced.stage.targets[0].reason_code,
        reason::FRESHEST_RESULT
    );
    Ok(())
}

#[test]
fn the_same_result_is_reduced_once_a_later_turn_follows() -> anyhow::Result<()> {
    let [call, result] = read_pair("t1", "/tmp/big.rs", json!({}), &big_file_content(200));
    let raw = raw_in_history(json!({"messages": [call, result]}))?;
    let tp = TestPolicy::default();
    let reduced = reduce_file_reads(&raw, &tp.policy());
    assert_eq!(reduced.stage.outcome, StageOutcome::Applied);
    assert_eq!(
        reduced.stage.targets[0].reason_code,
        reason::FILE_READ_REDUCED
    );
    Ok(())
}

#[test]
fn recovery_reread_of_a_marked_range_is_forwarded_whole_even_in_history() -> anyhow::Result<()> {
    let [call, result] = read_pair("t1", "/tmp/big.rs", json!({}), &big_file_content(200));
    let first_raw = raw_in_history(json!({"messages": [call, result]}))?;
    let tp = TestPolicy::default();
    let first = reduce_file_reads(&first_raw, &tp.policy());
    assert_eq!(first.stage.outcome, StageOutcome::Applied);

    let mut body: Value = serde_json::from_slice(&first.candidate)?;
    let recovered: String = (41..=180)
        .map(|n| format!("recovered line {n} with enough padding to matter\n"))
        .collect();
    let [recall, reresult] = read_pair(
        "t2",
        "/tmp/big.rs",
        json!({"offset": 41, "limit": 140}),
        &recovered,
    );
    if let Some(messages) = body["messages"].as_array_mut() {
        messages.push(recall);
        messages.push(reresult);
    }
    let raw = raw_in_history(body)?;
    let second = reduce_file_reads(&raw, &tp.policy());

    assert_eq!(second.candidate.as_ref(), raw.as_slice());
    let recovery = second
        .stage
        .targets
        .iter()
        .find(|t| t.tool_use_id.as_deref() == Some("t2"))
        .ok_or_else(|| anyhow::anyhow!("t2 target missing"))?;
    assert_eq!(recovery.outcome, StageOutcome::Skipped);
    assert_eq!(recovery.reason_code, reason::RECOVERY_READ);
    Ok(())
}

#[test]
fn an_explicit_range_stays_whole_without_a_matching_marker() -> anyhow::Result<()> {
    let [call, result] = read_pair("t1", "/tmp/big.rs", json!({}), &big_file_content(200));
    let first_raw = raw_in_history(json!({"messages": [call, result]}))?;
    let tp = TestPolicy::default();
    let first = reduce_file_reads(&first_raw, &tp.policy());

    let mut body: Value = serde_json::from_slice(&first.candidate)?;
    let [recall, reresult] = read_pair(
        "t2",
        "/tmp/other.rs",
        json!({"offset": 41, "limit": 140}),
        &big_file_content(150).replace("line ", "other line "),
    );
    if let Some(messages) = body["messages"].as_array_mut() {
        messages.push(recall);
        messages.push(reresult);
    }
    let raw = raw_in_history(body)?;
    let second = reduce_file_reads(&raw, &tp.policy());
    let other = second
        .stage
        .targets
        .iter()
        .find(|t| t.tool_use_id.as_deref() == Some("t2"))
        .ok_or_else(|| anyhow::anyhow!("t2 target missing"))?;
    assert_eq!(other.outcome, StageOutcome::Skipped);
    Ok(())
}

#[test]
fn ranged_reads_stay_whole_without_markers_after_compaction_or_replay() -> anyhow::Result<()> {
    for extra in [
        json!({"offset": 120, "limit": 180}),
        json!({"offset": 41}),
        json!({"limit": 265}),
    ] {
        let [call, result] =
            read_pair("recovered", "/tmp/file.ts", extra, &numbered_lines(41, 265));
        let raw = raw_in_history(json!({"messages": [call, result]}))?;
        let policy = TestPolicy::default();
        for _ in 0..2 {
            let reduced = reduce_file_reads(&raw, &policy.policy());
            assert_eq!(reduced.candidate.as_ref(), raw.as_slice());
            assert_eq!(reduced.stage.targets[0].reason_code, reason::RECOVERY_READ);
        }
    }
    Ok(())
}

#[test]
fn original_client_history_preserves_recovery_after_forwarded_only_reduction() -> anyhow::Result<()>
{
    let [call, result] = read_pair(
        "original",
        "/tmp/file.py",
        json!({}),
        &numbered_lines(1, 325),
    );
    let first = raw_in_history(json!({"messages": [call, result]}))?;
    let policy = TestPolicy::default();
    assert_eq!(
        reduce_file_reads(&first, &policy.policy()).stage.outcome,
        StageOutcome::Applied
    );
    let mut original: Value = serde_json::from_slice(&first)?;
    let recovered = numbered_lines(41, 265);
    let pair = read_pair(
        "recovered",
        "/tmp/file.py",
        json!({"offset":41,"limit":265}),
        &recovered,
    );
    original["messages"]
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("messages"))?
        .extend(pair);
    let raw = raw_in_history(original)?;
    let reduced = reduce_file_reads(&raw, &policy.policy());
    let sent: Value = serde_json::from_slice(&reduced.candidate)?;
    assert_eq!(sent["messages"][5]["content"][0]["content"], recovered);
    assert_eq!(
        reduce_file_reads(&raw, &policy.policy()).candidate,
        reduced.candidate
    );
    Ok(())
}

#[test]
fn a_full_file_read_keeps_marker_offsets_from_line_one() -> anyhow::Result<()> {
    let [call, result] = read_pair("t1", "/tmp/big.rs", json!({}), &numbered_lines(1, 200));
    let raw = raw_in_history(json!({"messages": [call, result]}))?;
    let tp = TestPolicy::default();
    let reduced = reduce_file_reads(&raw, &tp.policy());
    let marker = marker_line(&reduced.candidate)?;
    assert!(marker.contains("omitted=41-180 (140 lines)"), "{marker}");
    assert!(marker.contains("offset=41, limit=140"), "{marker}");
    Ok(())
}

#[test]
fn a_malformed_marker_is_not_needed_to_protect_a_ranged_read() -> anyhow::Result<()> {
    let bogus = format!("{REDUCTION_MARKER_TAG} file=/tmp/big.rs omitted=garbage");
    let [call, result] = read_pair("t1", "/tmp/big.rs", json!({}), &bogus);
    let [recall, reresult] = read_pair(
        "t2",
        "/tmp/big.rs",
        json!({"offset": 41, "limit": 140}),
        &big_file_content(200),
    );
    let raw = raw_in_history(json!({"messages": [call, result, recall, reresult]}))?;
    let tp = TestPolicy::default();
    let reduced = reduce_file_reads(&raw, &tp.policy());
    let later = reduced
        .stage
        .targets
        .iter()
        .find(|t| t.tool_use_id.as_deref() == Some("t2"))
        .ok_or_else(|| anyhow::anyhow!("t2 target missing"))?;
    assert_eq!(later.outcome, StageOutcome::Skipped);
    Ok(())
}
