use std::collections::BTreeSet;

use anyhow::{Context, Result};
use serde_json::{Value, json};

use super::followups::pick_sources;
use super::shell::{
    ShellFollowups, needed_tokens, parse_shell_cut, shell_harm_table, split_output,
};
use super::test_support::{arms, at, call, cut, primary, source, who};
use super::types::{Call, Cut, Gaps, Source, stamp};

const DETAIL: &str = "kept output lines 1-20,50-53,281-300 (44 of 300 lines, 1 signal lines) \
     in place of 2 omitted range(s); the exact original is recoverable";
const COMMAND: &str = "cargo test --lib cmd_only_token_x";
const T0: &str = "2026-11-01T10:00:00Z";

fn content() -> String {
    [
        "cargo test output start",
        "kept_line_token_alpha ok",
        "error in crates/mike/src/hidden_file.rs",
        "kept_line_token_alpha repeats",
        "failing commit deadbeef12",
        "ran cmd_only_token_x",
        "short ab",
        "tail one",
        "tail two",
    ]
    .join("\n")
}

fn shell_cut(detail: &str) -> Cut {
    cut(primary(1), "toolu_1", "shell_output_reduced", detail, T0)
}

fn default_cut() -> Cut {
    shell_cut("kept output lines 1-2,8-9 (4 of 9 lines)")
}

fn shell_source() -> Source {
    let mut found = source(
        primary(1),
        10,
        "toolu_1",
        None,
        Value::String(content()),
        "2026-11-01T10:00:01Z",
    );
    found.input = Some(json!({ "command": COMMAND }));
    found
}

fn later(id: i64, minutes: u32, text: &str, session: i64, agent: Option<&str>) -> Call {
    let origin = if agent.is_some() {
        "subagent"
    } else {
        "primary"
    };
    let mut made = call(
        who(session, None, agent, origin),
        id,
        "Read",
        None,
        &format!("2026-11-01T10:{minutes:02}:00Z"),
    );
    made.input = Some(json!({ "file_path": text }));
    made
}

fn table_for(
    calls: &[Call],
    arm: &str,
    until: &str,
    cuts: &[Cut],
    sources: Vec<Source>,
) -> (super::types::ShellTable, Gaps) {
    let assigned = arms(&[(1, arm)]);
    let found = pick_sources(sources);
    shell_harm_table(&ShellFollowups {
        cuts,
        arms: &assigned,
        sources: &found,
        calls,
        until: stamp(at(until)),
        window: 1800.0,
    })
}

fn default_table(calls: &[Call], arm: &str) -> (super::types::ShellTable, Gaps) {
    table_for(
        calls,
        arm,
        "2026-11-02T00:00:00Z",
        &[default_cut()],
        vec![shell_source()],
    )
}

#[test]
fn parse_shell_cut_reads_ranges_from_the_detail() {
    assert_eq!(
        parse_shell_cut(&shell_cut(DETAIL)),
        Some(vec![(1, 20), (50, 53), (281, 300)])
    );
}

#[test]
fn parse_shell_cut_single_line_is_a_range_of_one() {
    assert_eq!(
        parse_shell_cut(&shell_cut("kept output lines 7,9-9 (2 of 30 lines)")),
        Some(vec![(7, 7), (9, 9)])
    );
}

#[test]
fn parse_shell_cut_other_details_do_not_parse() {
    assert_eq!(
        parse_shell_cut(&shell_cut("file a.rs: kept file lines 1-2 and 3-4")),
        None
    );
    let mut empty = default_cut();
    empty.detail = None;
    assert_eq!(parse_shell_cut(&empty), None);
}

#[test]
fn split_output_uses_one_based_line_numbers_and_text_parts() -> Result<()> {
    let (hidden, shown) =
        split_output(&Value::String(content()), &[(1, 2), (8, 9)]).context("string content")?;
    assert!(hidden.contains("hidden_file.rs"));
    assert!(!shown.contains("hidden_file.rs"));
    assert!(shown.contains("tail two"));
    let parts = json!([{"type": "text", "text": "a\nb"}, {"type": "text", "text": "\nc"}]);
    assert_eq!(
        split_output(&parts, &[(2, 2)]),
        Some(("a\nc".to_string(), "b".to_string()))
    );
    assert_eq!(split_output(&Value::Null, &[(1, 1)]), None);
    Ok(())
}

#[test]
fn needed_tokens_are_only_those_the_kept_lines_and_command_lack() -> Result<()> {
    let (hidden, shown) =
        split_output(&Value::String(content()), &[(1, 2), (8, 9)]).context("string content")?;
    let expected: BTreeSet<String> = ["crates/mike/src/hidden_file.rs", "deadbeef12"]
        .into_iter()
        .map(str::to_string)
        .collect();
    assert_eq!(needed_tokens(&hidden, &shown, COMMAND), expected);
    Ok(())
}

#[test]
fn shell_harm_later_call_using_a_hidden_token_counts() {
    let (table, gaps) = default_table(
        &[later(20, 5, "crates/mike/src/hidden_file.rs", 1, None)],
        "forward",
    );
    assert_eq!((table.forward.cuts, table.forward.needed_later), (1, 1));
    assert_eq!(table.control.cuts, 0);
    assert!(gaps.is_empty());
}

#[test]
fn shell_harm_token_already_in_the_kept_lines_does_not_count() {
    let calls = [
        later(20, 5, "kept_line_token_alpha", 1, None),
        later(21, 6, "cmd_only_token_x", 1, None),
    ];
    let (table, _) = default_table(&calls, "forward");
    assert_eq!((table.forward.cuts, table.forward.needed_later), (1, 0));
}

#[test]
fn shell_harm_calls_out_of_window_or_lane_do_not_count() {
    let elsewhere = [
        later(20, 5, "deadbeef12", 1, Some("sub")),
        later(21, 5, "deadbeef12", 2, None),
        later(22, 45, "deadbeef12", 1, None),
    ];
    let (table, _) = default_table(&elsewhere, "forward");
    assert_eq!(table.forward.needed_later, 0);
    let (before_receipt, _) = default_table(&[later(23, 0, "deadbeef12", 1, None)], "forward");
    assert_eq!(before_receipt.forward.needed_later, 0);
}

#[test]
fn shell_harm_arm_decides_the_row_and_a_wire_id_counts_once() {
    let (table, _) = table_for(
        &[later(20, 5, "deadbeef12", 1, None)],
        "control",
        "2026-11-02T00:00:00Z",
        &[default_cut(), default_cut()],
        vec![shell_source()],
    );
    assert_eq!((table.control.cuts, table.control.needed_later), (1, 1));
    assert_eq!(table.forward.cuts, 0);
}

#[test]
fn shell_harm_right_censored_cut_is_left_out() {
    let (table, gaps) = table_for(
        &[later(20, 5, "deadbeef12", 1, None)],
        "forward",
        "2026-11-01T10:10:00Z",
        &[default_cut()],
        vec![shell_source()],
    );
    assert_eq!(table.forward.cuts, 0);
    assert_eq!(gaps, Gaps::from([("right_censored".to_string(), 1)]));
}

#[test]
fn shell_harm_cut_without_a_source_is_a_gap() {
    let (table, gaps) = table_for(
        &[],
        "forward",
        "2026-11-02T00:00:00Z",
        &[default_cut()],
        Vec::new(),
    );
    assert_eq!(table.forward.cuts, 0);
    assert_eq!(
        gaps,
        Gaps::from([("missing_source_or_policy".to_string(), 1)])
    );
}
