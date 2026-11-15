use std::collections::HashMap;

use sqlx::PgPool;
use tracing::warn;

use crate::store::persistence::turn_tool_call_reads::TurnToolCallMetaRow;

#[cfg(test)]
#[path = "trajectory_digest_tests.rs"]
mod trajectory_digest_tests;

const NO_OP_OPERATIONS: &[&str] = &["read", "search", "fetch"];

const DIGEST_MAX_CHARS: usize = 8_000;
const DROPPED_NOTE: &str = "... (earlier tool calls dropped at the digest cap)";
const TARGET_CHARS: usize = 120;
const RESULT_HEAD_CHARS: usize = 80;
const FOLDED_TARGETS: usize = 3;

const DETAILS_SQL: &str = "SELECT tu.wire_tool_use_id,
        left(COALESCE(tu.input->>'command', tu.input->>'file_path', tu.input->>'notebook_path',
                      tu.input->>'path', tu.input->>'pattern', tu.input->>'url'), 600),
        left(CASE jsonb_typeof(res.content)
                  WHEN 'string' THEN res.content #>> '{}'
                  WHEN 'array' THEN res.content -> 0 ->> 'text'
             END, 600)
   FROM tools.uses tu
   JOIN requests.requests r ON r.id = tu.request_id
   LEFT JOIN tools.results res ON res.tool_use_id = tu.id
  WHERE r.operator_turn_id = $1
    AND tu.session_id = $2";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolCallDetail {
    pub target: Option<String>,
    pub result_head: Option<String>,
}

fn cut(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

pub fn detail_from(target: Option<&str>, result: Option<&str>) -> ToolCallDetail {
    ToolCallDetail {
        target: target
            .map(|t| {
                cut(
                    &t.split_whitespace().collect::<Vec<_>>().join(" "),
                    TARGET_CHARS,
                )
            })
            .filter(|t| !t.is_empty()),
        result_head: result
            .and_then(|r| r.lines().map(str::trim).find(|l| !l.is_empty()))
            .map(|l| cut(l, RESULT_HEAD_CHARS)),
    }
}

pub async fn fetch_tool_call_details(
    pool: &PgPool,
    turn_id: i64,
    session_db_id: i64,
) -> HashMap<String, ToolCallDetail> {
    let rows: Result<Vec<(String, Option<String>, Option<String>)>, sqlx::Error> =
        sqlx::query_as(DETAILS_SQL)
            .bind(turn_id)
            .bind(session_db_id)
            .fetch_all(pool)
            .await;
    match rows {
        Ok(rows) => rows
            .into_iter()
            .map(|(id, target, result)| (id, detail_from(target.as_deref(), result.as_deref())))
            .collect(),
        Err(error) => {
            warn!(
                %error,
                turn_id,
                "trajectory digest: tool call details read failed; the digest names no commands or results"
            );
            HashMap::new()
        }
    }
}

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

fn target_of<'a>(
    row: &TurnToolCallMetaRow,
    details: &'a HashMap<String, ToolCallDetail>,
) -> Option<&'a str> {
    details.get(&row.tool_id)?.target.as_deref()
}

fn folded_line(
    run: &[TurnToolCallMetaRow],
    start: usize,
    details: &HashMap<String, ToolCallDetail>,
) -> String {
    let reqs = match (run.first(), run.last()) {
        (Some(a), Some(b)) if a.request_id != b.request_id => {
            format!("{}-{}", a.request_id, b.request_id)
        }
        (Some(a), _) => a.request_id.to_string(),
        _ => String::new(),
    };
    let mut line = format!(
        "{}. {} no-op read(s) (ok) [req {reqs}]",
        start + 1,
        run.len()
    );
    let targets: Vec<&str> = run.iter().filter_map(|r| target_of(r, details)).collect();
    if !targets.is_empty() {
        let shown: Vec<&str> = targets.iter().take(FOLDED_TARGETS).copied().collect();
        line.push_str(": ");
        line.push_str(&shown.join(", "));
        let more = targets.len().saturating_sub(FOLDED_TARGETS);
        if more > 0 {
            line.push_str(&format!(", +{more} more"));
        }
    }
    line
}

fn call_line(
    row: &TurnToolCallMetaRow,
    index: usize,
    details: &HashMap<String, ToolCallDetail>,
) -> String {
    let op = row.operation.as_deref().unwrap_or("other");
    let detail = details.get(&row.tool_id);
    let target = detail
        .and_then(|d| d.target.as_deref())
        .map_or_else(String::new, |t| format!(" `{t}`"));
    let head = detail
        .filter(|_| row.has_result)
        .and_then(|d| d.result_head.as_deref())
        .map_or_else(String::new, |h| format!(": {h}"));
    format!(
        "{}. [req {}] {} ({op}){target} → {}{head} [tool {}]",
        index + 1,
        row.request_id,
        row.tool_name,
        tool_outcome_label(row),
        row.tool_id,
    )
}

fn digest_lines(
    rows: &[TurnToolCallMetaRow],
    details: &HashMap<String, ToolCallDetail>,
) -> Vec<String> {
    let mut lines = Vec::with_capacity(rows.len());
    let mut i = 0;
    while i < rows.len() {
        let start = i;
        if is_no_op_success(&rows[i]) {
            while i < rows.len() && is_no_op_success(&rows[i]) {
                i += 1;
            }
            lines.push(folded_line(&rows[start..i], start, details));
        } else {
            lines.push(call_line(&rows[i], i, details));
            i += 1;
        }
    }
    lines
}

fn newest_within_cap(lines: &[String]) -> String {
    let all: usize = lines.iter().map(|l| l.len() + 1).sum();
    let mut out = String::with_capacity(all.min(DIGEST_MAX_CHARS));
    let kept = if all <= DIGEST_MAX_CHARS {
        lines.len()
    } else {
        let budget = DIGEST_MAX_CHARS - DROPPED_NOTE.len() - 1;
        let mut used = 0;
        out.push_str(DROPPED_NOTE);
        out.push('\n');
        lines
            .iter()
            .rev()
            .take_while(|l| {
                used += l.len() + 1;
                used <= budget
            })
            .count()
    };
    for line in &lines[lines.len() - kept..] {
        out.push_str(line);
        out.push('\n');
    }
    out
}

pub fn build_trajectory_digest(
    rows: &[TurnToolCallMetaRow],
    details: &HashMap<String, ToolCallDetail>,
) -> String {
    if rows.is_empty() {
        return String::new();
    }
    newest_within_cap(&digest_lines(rows, details))
}
