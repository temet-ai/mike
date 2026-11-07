use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::store::persistence::fleet::{BackgroundEnd, TreeResultRow};

const BACKGROUND_REPLY: &str = "Command running in background";

fn result_text(content: &serde_json::Value) -> String {
    match content {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| b.get("text").and_then(serde_json::Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

pub fn launched_in_background(row: &TreeResultRow) -> bool {
    row.content
        .as_ref()
        .is_some_and(|c| result_text(c).starts_with(BACKGROUND_REPLY))
}

pub fn completion_unknown(row: &TreeResultRow) -> bool {
    row.finished_at.is_none() && launched_in_background(row)
}

fn settle_one(mut row: TreeResultRow, ends: &BTreeMap<i64, &BackgroundEnd>) -> TreeResultRow {
    if !launched_in_background(&row) {
        return row;
    }
    match ends.get(&row.use_id) {
        Some(end) if end.state != "running" && end.finished_at.is_some() => {
            row.finished_at = end.finished_at;
            row.is_error = Some(end.state != "completed");
        }
        _ => {
            row.finished_at = None;
            row.is_error = None;
        }
    }
    row
}

pub fn settle_background(
    rows: Vec<TreeResultRow>,
    ends: &[BackgroundEnd],
    _now: DateTime<Utc>,
) -> Vec<TreeResultRow> {
    let by_use: BTreeMap<i64, &BackgroundEnd> = ends.iter().map(|e| (e.use_id, e)).collect();
    rows.into_iter()
        .map(|row| settle_one(row, &by_use))
        .collect()
}
