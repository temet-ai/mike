use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};

use super::work_of::work_through;
use crate::store::persistence::fleet::{BackgroundEnd, TreeResultRow};

#[path = "build_target.rs"]
mod target;

pub const MEMORY_FLOOR_MB: u64 = 6144;
const MEMINFO: &str = "/proc/meminfo";
const BACKGROUND_REPLY: &str = "Command running in background";
const HEAVY_VERBS: [&str; 9] = [
    "build",
    "check",
    "test",
    "clippy",
    "nextest",
    "lint",
    "gate",
    "sqlx-prepare",
    "prepare",
];
const HEAVY_PROGRAMS: [&str; 3] = ["cargo", "cx", "make"];
const HEAVY_SCRIPTS: [&str; 3] = ["sqlx-prepare", "gate", "lint-rust"];

fn heavy_verb(word: &str) -> bool {
    HEAVY_VERBS.iter().any(|verb| {
        word == *verb
            || word
                .strip_prefix(verb)
                .is_some_and(|rest| rest.starts_with('-'))
    })
}

fn heavy_item(item: &str) -> bool {
    let mut words = item.split_whitespace();
    let Some(program) = words.next() else {
        return false;
    };
    let base = program.rsplit('/').next().unwrap_or(program);
    if HEAVY_SCRIPTS.contains(&base) {
        return true;
    }
    HEAVY_PROGRAMS.contains(&base) && words.take(3).any(heavy_verb)
}

pub fn is_heavy(command: &str) -> bool {
    work_through(command, &[]).iter().any(|w| heavy_item(w))
        || target::work(command).is_some_and(|work| heavy_item(&work))
}

pub fn parse_available_mb(meminfo: &str) -> Option<u64> {
    let line = meminfo.lines().find(|l| l.starts_with("MemAvailable:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024)
}

pub fn available_mb() -> Option<u64> {
    parse_available_mb(&std::fs::read_to_string(MEMINFO).ok()?)
}

pub fn running_allowed(slots: usize, available_mb: Option<u64>, floor_mb: u64) -> usize {
    match available_mb {
        Some(free) if free < floor_mb => 1,
        _ => slots.max(1),
    }
}

pub fn queued(rows: &[TreeResultRow], allowed: usize) -> BTreeSet<i64> {
    let mut open: Vec<(&TreeResultRow, String)> = rows
        .iter()
        .filter(|r| r.finished_at.is_none() && r.started_at.is_some())
        .filter(|r| !completion_unknown(r))
        .filter_map(|row| {
            let command = row.command.as_deref()?;
            is_heavy(command).then_some(())?;
            Some((row, target::directory(command)?))
        })
        .collect();
    open.sort_by_key(|(row, _)| (row.started_at, row.use_id));
    let mut counts = BTreeMap::<String, usize>::new();
    let mut waiting = BTreeSet::new();
    for (row, directory) in open {
        let count = counts.entry(directory).or_default();
        if *count >= allowed.max(1) {
            waiting.insert(row.use_id);
        }
        *count += 1;
    }
    waiting
}

pub fn waiting_without_wrapper(rows: &[TreeResultRow]) -> BTreeSet<i64> {
    queued(rows, running_allowed(1, available_mb(), MEMORY_FLOOR_MB))
}

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
