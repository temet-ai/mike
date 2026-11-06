use super::{SectionCaps, char_len, cut_head};
use crate::store::persistence::session_action_list::{SessionActionCounts, SessionActionRow};

const ACTION_HEADER_RESERVE: usize = 220;

fn is_successful_no_op(row: &SessionActionRow) -> bool {
    matches!(row.operation.as_deref(), Some("read" | "search" | "fetch"))
        && row.has_result
        && row.result_is_error != Some(true)
}

fn action_outcome(row: &SessionActionRow) -> &'static str {
    if !row.has_result {
        "no result"
    } else if row.result_is_error == Some(true) {
        "error"
    } else {
        "ok"
    }
}

fn action_line(position: usize, row: &SessionActionRow, detail_cap: usize) -> String {
    let outcome = action_outcome(row);
    let detail = row
        .command
        .as_deref()
        .or(row.file_path.as_deref())
        .unwrap_or_default();
    if detail.is_empty() {
        format!("{position}. {} → {outcome}", row.tool_name)
    } else {
        format!(
            "{position}. {} — {} → {outcome}",
            row.tool_name,
            cut_head(detail, detail_cap)
        )
    }
}

struct ActionItem {
    rows: usize,
    line: String,
}

fn fold_action_items(
    rows: &[SessionActionRow],
    offset: usize,
    detail_cap: usize,
) -> Vec<ActionItem> {
    let mut items = Vec::new();
    let mut i = 0;
    while i < rows.len() {
        let mut j = i;
        while j < rows.len() && is_successful_no_op(&rows[j]) {
            j += 1;
        }
        if j - i >= 2 {
            items.push(ActionItem {
                rows: j - i,
                line: format!(
                    "{}-{}. {} read/search/fetch calls → ok",
                    offset + i + 1,
                    offset + j,
                    j - i
                ),
            });
            i = j;
        } else {
            items.push(ActionItem {
                rows: 1,
                line: action_line(offset + i + 1, &rows[i], detail_cap),
            });
            i += 1;
        }
    }
    items
}

fn first_shown_item(items: &[ActionItem], budget_chars: usize) -> usize {
    let mut used = 0;
    let mut start = items.len();
    for (i, item) in items.iter().enumerate().rev() {
        let need = char_len(&item.line) + 1;
        if used + need > budget_chars && start < items.len() {
            break;
        }
        used += need;
        start = i;
    }
    start
}

pub(super) fn tally_counts(rows: &[SessionActionRow]) -> SessionActionCounts {
    let count = |f: &dyn Fn(&SessionActionRow) -> bool| rows.iter().filter(|r| f(r)).count() as i64;
    SessionActionCounts {
        total: rows.len() as i64,
        reads: count(&|r| matches!(r.operation.as_deref(), Some("read" | "search" | "fetch"))),
        writes: count(&|r| r.operation.as_deref() == Some("write")),
        commands: count(&|r| r.tool_name == "Bash"),
        failed_commands: count(&|r| r.tool_name == "Bash" && r.result_is_error == Some(true)),
    }
}

fn omitted_line(
    total: usize,
    shown_rows: usize,
    counts: &SessionActionCounts,
    shown: &[SessionActionRow],
) -> String {
    let seen = tally_counts(shown);
    let omitted = total - shown_rows;
    let writes = (counts.writes - seen.writes).max(0);
    let commands = (counts.commands - seen.commands).max(0);
    let failed = (counts.failed_commands - seen.failed_commands).max(0);
    let reads = (counts.reads - seen.reads).max(0);
    let other = (omitted as i64 - writes - commands - reads).max(0);
    format!(
        "({omitted} earlier calls not shown: {writes} writes, {commands} commands of which {failed} failed, {reads} reads, searches or fetches, {other} other)"
    )
}

pub(super) fn project_action_list(
    rows: &[SessionActionRow],
    counts: Option<SessionActionCounts>,
    caps: &SectionCaps,
) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let counts = counts.unwrap_or_else(|| tally_counts(rows));
    let total = usize::try_from(counts.total).unwrap_or(0).max(rows.len());
    let offset = total - rows.len();
    let items = fold_action_items(rows, offset, caps.action_detail);
    let mut start = first_shown_item(&items, caps.session_actions);
    if start > 0 || offset > 0 {
        start = first_shown_item(
            &items,
            caps.session_actions.saturating_sub(ACTION_HEADER_RESERVE),
        );
    }
    let shown_rows: usize = items[start..].iter().map(|i| i.rows).sum();
    let mut out = String::new();
    if total > shown_rows {
        let shown = &rows[rows.len() - shown_rows..];
        out.push_str(&omitted_line(total, shown_rows, &counts, shown));
        out.push('\n');
    }
    for item in &items[start..] {
        out.push_str(&item.line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
#[path = "action_list_tests.rs"]
mod tests;
