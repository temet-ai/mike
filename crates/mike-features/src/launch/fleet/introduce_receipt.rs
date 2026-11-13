use serde_json::{Value, json};

use super::Repeat;
use crate::launch::fleet::report_proof;
use crate::store::persistence::fleet::{ChangedPathRow, TreeResultRow};

fn output_reference(row: &TreeResultRow) -> Value {
    let proof = report_proof::from_rows(std::slice::from_ref(row), None);
    proof.receipts.into_iter().next().map_or_else(
        || json!({
            "tool": "list_session_tool_uses",
            "arguments": {"id": row.session_uuid, "include_results": true, "offset": 0},
            "find_use_id": row.use_id,
            "pagination": "advance offset until this use_id; result.content is the full captured output",
        }),
        |receipt| receipt.full_output,
    )
}

pub fn handoff(repeat: &Repeat, changed: &[ChangedPathRow]) -> Value {
    let first = &repeat.first;
    let row = TreeResultRow {
        use_id: first.use_id,
        seat: Some(first.seat.clone()),
        session_uuid: Some(first.session),
        command: Some(first.command.clone()),
        started_at: Some(first.started_at),
        finished_at: (!first.in_background).then_some(first.finished_at),
        is_error: (!first.in_background).then_some(first.is_error),
        content: None,
    };
    let changed_paths = changed
        .iter()
        .filter(|change| change.seen_at >= first.started_at)
        .map(|change| change.path.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let still_valid = report_proof::captured_result_validity(&row, Some(changed));
    let outcome = match (first.in_background, first.is_error) {
        (true, _) => "background launch; completion unknown",
        (false, true) => "tool reported an error",
        (false, false) => "tool completed without an error; test or lint verdict not inferred",
    };
    json!({
        "source": "captured Bash tool result",
        "use_id": first.use_id,
        "repeated_by_use_id": repeat.again.use_id,
        "session": first.session,
        "seat": first.seat,
        "command_excerpt": first.command.chars().take(192).collect::<String>(),
        "command_truncated": first.command.chars().count() > 192,
        "started_at": first.started_at,
        "finished_at": row.finished_at,
        "tool_outcome": outcome,
        "is_error": row.is_error,
        "commit": null,
        "commit_provenance": "captured Bash results do not record a commit binding",
        "still_valid": still_valid,
        "validity_provenance": "all observed checkout paths since command start; event coverage and command dependency closure are unknown",
        "observed_changed_path_count": changed_paths.len(),
        "first_measured_seconds": repeat.first_seconds,
        "repeated_measured_seconds": repeat.again_seconds,
        "full_command_and_output": output_reference(&row),
    })
}
