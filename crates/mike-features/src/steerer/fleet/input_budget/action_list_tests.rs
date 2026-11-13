use super::super::{V1_CAPS, V1_WIDE_CAPS};
use super::*;
use crate::store::persistence::session_action_list::SessionActionRow;

fn chars(text: &str) -> usize {
    text.chars().count()
}

fn counts_of(rows: &[SessionActionRow]) -> SessionActionCounts {
    tally_counts(rows)
}

fn row(tool: &str, operation: &str, detail: Option<&str>, ok: bool) -> SessionActionRow {
    let is_bash = tool == "Bash";
    SessionActionRow {
        tool_name: tool.to_string(),
        file_path: if is_bash {
            None
        } else {
            detail.map(str::to_string)
        },
        command: if is_bash {
            detail.map(str::to_string)
        } else {
            None
        },
        has_result: true,
        result_is_error: Some(!ok),
        operation: Some(operation.to_string()),
    }
}

fn mixed_rows(n: usize) -> Vec<SessionActionRow> {
    (0..n)
        .map(|i| match i % 4 {
            0 => row("Edit", "write", Some(&format!("/src/f{i}.rs")), true),
            1 => row("Bash", "exec", Some(&format!("cargo test {i}")), i % 8 != 1),
            2 => row("Agent", "dispatch", None, true),
            _ => row("Bash", "exec", Some("ls"), true),
        })
        .collect()
}

#[test]
fn action_projection_first_line_counts_writes_commands_failed_reads() {
    let all = mixed_rows(400);
    let counts = counts_of(&all);
    let out = project_action_list(&all[200..], Some(counts), &V1_CAPS);
    let first = out.lines().next().unwrap_or_default();
    assert!(first.starts_with("(") && first.contains(" earlier calls not shown: "));
    let shown_rows = out.lines().skip(1).count();
    let head_count: usize = first
        .trim_start_matches('(')
        .split(' ')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    assert_eq!(head_count + shown_rows, 400);
    assert!(
        first.contains("writes,")
            && first.contains("commands of which")
            && first.contains("failed,")
    );
    assert!(first.contains("reads, searches or fetches,") && first.ends_with("other)"));
    assert!(chars(&out) <= 4_000);
}

#[test]
fn action_projection_numbers_rows_by_session_position() {
    let all = mixed_rows(50);
    let out = project_action_list(&all[30..], Some(counts_of(&all)), &V1_CAPS);
    let last = out.lines().last().unwrap_or_default();
    assert!(
        last.starts_with("50. "),
        "last row carries the session total: {last}"
    );
    assert!(out.contains("\n31. "));
    assert!(out.starts_with("(30 earlier calls not shown: "));
}

#[test]
fn action_projection_folds_successful_read_runs_only() {
    let mut rows = vec![row("Edit", "write", Some("/a.rs"), true)];
    for i in 0..5 {
        rows.push(row("Read", "read", Some(&format!("/r{i}.rs")), true));
    }
    rows.push(row("Bash", "exec", Some("cargo test"), true));
    rows.push(row("Read", "read", Some("/single.rs"), true));
    rows.push(row("Bash", "exec", Some("ls"), true));
    let out = project_action_list(&rows, Some(counts_of(&rows)), &V1_CAPS);
    assert!(out.contains("2-6. 5 read/search/fetch calls → ok"));
    assert!(out.contains("8. Read — /single.rs → ok"));
    assert!(!out.contains("earlier calls not shown"));
    assert_eq!(out.lines().count(), 5);
}

#[test]
fn action_projection_keeps_failed_read_unfolded() {
    let rows = vec![
        row("Read", "read", Some("/a.rs"), true),
        row("Read", "read", Some("/b.rs"), false),
        row("Read", "read", Some("/c.rs"), true),
        row("Read", "read", Some("/d.rs"), true),
    ];
    let out = project_action_list(&rows, Some(counts_of(&rows)), &V1_CAPS);
    assert!(out.contains("1. Read — /a.rs → ok"));
    assert!(out.contains("2. Read — /b.rs → error"));
    assert!(out.contains("3-4. 2 read/search/fetch calls → ok"));
}

#[test]
fn bash_command_over_detail_cap_is_head_cut() {
    let cmd = format!("echo {}", "x".repeat(1_000));
    let rows = vec![row("Bash", "exec", Some(&cmd), true)];
    let out = project_action_list(&rows, Some(counts_of(&rows)), &V1_CAPS);
    assert!(out.starts_with("1. Bash — echo xxx"));
    assert!(out.contains(" more characters] → ok"));
    let detail = out
        .trim_start_matches("1. Bash — ")
        .trim_end_matches(" → ok\n");
    assert!(chars(detail) <= 240);
    let wide = project_action_list(&rows, Some(counts_of(&rows)), &V1_WIDE_CAPS);
    assert!(chars(&wide) > chars(&out));
}
