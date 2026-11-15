use std::collections::HashMap;

use super::*;

fn make_paired_row(
    tool_id: &str,
    tool_name: &str,
    request_id: i64,
    operation: Option<&str>,
    has_result: bool,
    result_is_error: Option<bool>,
) -> TurnToolCallMetaRow {
    TurnToolCallMetaRow {
        tool_id: tool_id.to_string(),
        tool_name: tool_name.to_string(),
        input_bytes: Some(64),
        ord: 0,
        request_id,
        has_result,
        result_is_error,
        result_content_bytes: Some(128),
        operation: operation.map(str::to_string),
    }
}

fn none() -> HashMap<String, ToolCallDetail> {
    HashMap::new()
}

fn details(entries: &[(&str, Option<&str>, Option<&str>)]) -> HashMap<String, ToolCallDetail> {
    entries
        .iter()
        .map(|(id, target, result)| ((*id).to_owned(), detail_from(*target, *result)))
        .collect()
}

#[test]
fn digest_empty_for_no_tool_calls() {
    assert_eq!(build_trajectory_digest(&[], &none()), "");
}

#[test]
fn digest_compresses_consecutive_successful_reads() {
    let rows = vec![
        make_paired_row("t1", "Read", 100, Some("read"), true, Some(false)),
        make_paired_row("t2", "Read", 100, Some("read"), true, Some(false)),
        make_paired_row("t3", "Grep", 101, Some("search"), true, Some(false)),
    ];
    let digest = build_trajectory_digest(&rows, &none());
    assert_eq!(
        digest.lines().count(),
        1,
        "all three no-ops fold into one line"
    );
    assert!(digest.contains("3 no-op read(s) (ok)"));
    assert!(digest.contains("[req 100-101]"));
}

#[test]
fn digest_gives_write_and_exec_their_own_line() {
    let rows = vec![
        make_paired_row("t1", "Read", 100, Some("read"), true, Some(false)),
        make_paired_row("t2", "Edit", 100, Some("write"), true, Some(false)),
        make_paired_row("t3", "Bash", 101, Some("exec"), true, Some(true)),
    ];
    let digest = build_trajectory_digest(&rows, &none());
    assert_eq!(
        digest.lines().count(),
        3,
        "the read stays a separate run since it does not neighbor another no-op"
    );
    assert!(digest.contains("Edit (write) → ok [tool t2]"));
    assert!(digest.contains("Bash (exec) → error [tool t3]"));
}

#[test]
fn digest_treats_erroring_read_as_non_compressible() {
    let rows = vec![make_paired_row(
        "t1",
        "Read",
        100,
        Some("read"),
        true,
        Some(true),
    )];
    let digest = build_trajectory_digest(&rows, &none());
    assert!(
        digest.contains("Read (read) → error"),
        "a failed read is a dead-end signal, not noise to fold away"
    );
}

#[test]
fn digest_treats_unknown_operation_as_non_compressible() {
    let rows = vec![make_paired_row(
        "t1",
        "McpCustomTool",
        100,
        None,
        true,
        Some(false),
    )];
    let digest = build_trajectory_digest(&rows, &none());
    assert!(digest.contains("McpCustomTool (other) → ok [tool t1]"));
}

#[test]
fn digest_covers_every_row_by_construction() {
    let rows: Vec<TurnToolCallMetaRow> = (0..40)
        .map(|i| {
            make_paired_row(
                &format!("t{i}"),
                "Read",
                100 + i,
                Some("read"),
                true,
                Some(false),
            )
        })
        .collect();
    let digest = build_trajectory_digest(&rows, &none());
    assert!(digest.contains("40 no-op read(s)"));
}

#[test]
fn digest_line_names_the_command_and_the_first_result_line() {
    let rows = vec![
        make_paired_row("t1", "Bash", 100, Some("exec"), true, Some(false)),
        make_paired_row("t2", "Edit", 101, Some("write"), true, Some(false)),
        make_paired_row("t3", "Bash", 102, Some("exec"), false, None),
    ];
    let long = format!("gh pr merge 306 --squash\n  && {}", "x".repeat(300));
    let found = details(&[
        (
            "t1",
            Some(long.as_str()),
            Some("\n✓ Squashed and merged pull request #306\nmore"),
        ),
        (
            "t2",
            Some("crates/a.rs"),
            Some("The file crates/a.rs has been updated."),
        ),
        ("t3", Some("scripts/deploy-proxy.sh"), Some("ignored")),
    ]);
    let digest = build_trajectory_digest(&rows, &found);
    let lines: Vec<&str> = digest.lines().collect();
    assert!(lines[0].contains("Bash (exec) `gh pr merge 306 --squash && xxx"));
    assert!(lines[0].contains("→ ok: ✓ Squashed and merged pull request #306 [tool t1]"));
    assert!(!lines[0].contains("more"));
    assert!(lines[1].contains(
        "Edit (write) `crates/a.rs` → ok: The file crates/a.rs has been updated. [tool t2]"
    ));
    assert!(lines[2].ends_with("`scripts/deploy-proxy.sh` → no result [tool t3]"));
}

#[test]
fn detail_from_cuts_the_command_and_the_result_line() {
    let spaced = "a ".repeat(200);
    let result = "r".repeat(200);
    let detail = detail_from(Some(spaced.as_str()), Some(result.as_str()));
    assert_eq!(
        detail.target.as_deref().map(|t| t.chars().count()),
        Some(TARGET_CHARS)
    );
    assert_eq!(
        detail.result_head.as_deref().map(|t| t.chars().count()),
        Some(RESULT_HEAD_CHARS)
    );
    assert_eq!(
        detail_from(Some("  "), Some("\n\n")),
        ToolCallDetail::default()
    );
}

#[test]
fn digest_folded_reads_list_their_paths() {
    let rows: Vec<TurnToolCallMetaRow> = (0..5)
        .map(|i| {
            make_paired_row(
                &format!("t{i}"),
                "Read",
                100,
                Some("read"),
                true,
                Some(false),
            )
        })
        .collect();
    let found = details(&[
        ("t0", Some("a.rs"), None),
        ("t1", Some("b.rs"), None),
        ("t2", Some("c.rs"), None),
        ("t3", Some("d.rs"), None),
    ]);
    let digest = build_trajectory_digest(&rows, &found);
    assert!(digest.contains("5 no-op read(s) (ok) [req 100]: a.rs, b.rs, c.rs, +1 more"));
}

#[test]
fn digest_over_the_cap_drops_the_oldest_lines_whole() {
    let rows: Vec<TurnToolCallMetaRow> = (0..2000)
        .map(|i| {
            make_paired_row(
                &format!("t{i}"),
                "Edit",
                100 + i,
                Some("write"),
                true,
                Some(false),
            )
        })
        .collect();
    let digest = build_trajectory_digest(&rows, &none());
    assert!(digest.len() <= DIGEST_MAX_CHARS);
    assert!(digest.starts_with(DROPPED_NOTE));
    assert!(digest.trim_end().ends_with("[tool t1999]"));
    assert!(!digest.contains("[tool t0]"));
    assert!(
        digest
            .lines()
            .skip(1)
            .all(|l| l.contains("Edit (write) → ok [tool t"))
    );
}
