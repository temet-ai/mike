use crate::store::persistence::turn_tool_call_reads::TurnToolCallMetaRow;

const NO_OP_OPERATIONS: &[&str] = &["read", "search", "fetch"];

const DIGEST_MAX_CHARS: usize = 8_000;

fn is_no_op_success(row: &TurnToolCallMetaRow) -> bool {
    matches!(row.operation.as_deref(), Some(op) if NO_OP_OPERATIONS.contains(&op))
        && row.has_result
        && row.result_is_error != Some(true)
}

fn tool_outcome_label(row: &TurnToolCallMetaRow) -> &'static str {
    if !row.has_result {
        "no result"
    } else if row.result_is_error == Some(true) {
        "error"
    } else {
        "ok"
    }
}

pub(crate) fn build_trajectory_digest(rows: &[TurnToolCallMetaRow]) -> String {
    if rows.is_empty() {
        return String::new();
    }

    let mut lines: Vec<String> = Vec::with_capacity(rows.len());
    let mut i = 0;
    while i < rows.len() {
        if is_no_op_success(&rows[i]) {
            let start = i;
            while i < rows.len() && is_no_op_success(&rows[i]) {
                i += 1;
            }
            let count = i - start;
            let first_req = rows[start].request_id;
            let last_req = rows[i - 1].request_id;
            lines.push(if first_req == last_req {
                format!(
                    "{}. {count} no-op read(s) (ok) [req {first_req}]",
                    start + 1
                )
            } else {
                format!(
                    "{}. {count} no-op read(s) (ok) [req {first_req}-{last_req}]",
                    start + 1
                )
            });
        } else {
            let row = &rows[i];
            let outcome = tool_outcome_label(row);
            let op = row.operation.as_deref().unwrap_or("other");
            lines.push(format!(
                "{}. [req {}] {} ({op}) → {outcome} [tool {}]",
                i + 1,
                row.request_id,
                row.tool_name,
                row.tool_id,
            ));
            i += 1;
        }
    }

    let mut out = String::with_capacity((lines.len() * 40).min(DIGEST_MAX_CHARS));
    for line in &lines {
        if out.len() + line.len() + 1 > DIGEST_MAX_CHARS {
            out.push_str("... (digest truncated at token cap)");
            break;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn digest_empty_for_no_tool_calls() {
        assert_eq!(build_trajectory_digest(&[]), "");
    }

    #[test]
    fn digest_compresses_consecutive_successful_reads() {
        let rows = vec![
            make_paired_row("t1", "Read", 100, Some("read"), true, Some(false)),
            make_paired_row("t2", "Read", 100, Some("read"), true, Some(false)),
            make_paired_row("t3", "Grep", 101, Some("search"), true, Some(false)),
        ];
        let digest = build_trajectory_digest(&rows);
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
        let digest = build_trajectory_digest(&rows);
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
        let digest = build_trajectory_digest(&rows);
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
        let digest = build_trajectory_digest(&rows);
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
        let digest = build_trajectory_digest(&rows);
        assert!(digest.contains("40 no-op read(s)"));
    }

    #[test]
    fn digest_truncates_at_char_cap_rather_than_growing_unbounded() {
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
        let digest = build_trajectory_digest(&rows);
        assert!(digest.len() <= DIGEST_MAX_CHARS + "... (digest truncated at token cap)".len());
        assert!(digest.contains("truncated at token cap"));
    }
}
