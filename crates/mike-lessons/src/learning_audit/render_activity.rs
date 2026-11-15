use std::collections::BTreeMap;

use serde_json::Value;

use super::findings::memory_grammar_totals;
use super::rows::{int, rate_text, rows, text};

pub(super) fn render_proposer_days(data: &Value) -> Vec<String> {
    let Some(days) = data.get("proposer_active_days").and_then(Value::as_array) else {
        return Vec::new();
    };
    if days.is_empty() {
        return vec!["- No lesson proposer call in the last three UTC months.".to_owned()];
    }
    days.iter()
        .map(|row| {
            format!(
                "- Days with at least one lesson proposer call, UTC month {}: {}.",
                text(row, "month"),
                text(row, "days_with_calls")
            )
        })
        .collect()
}

fn by_project(data: &Value, name: &str) -> BTreeMap<String, Value> {
    rows(data, name)
        .iter()
        .map(|row| (text(row, "project"), row.clone()))
        .collect()
}

fn grammar_table(data: &Value) -> Vec<String> {
    let recall = by_project(data, "memory_recall_tags");
    let update = by_project(data, "memory_update_tags");
    if recall.is_empty() && update.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![
        String::new(),
        "| Project | Recall tags | Recall delivered | Update tags found | Lessons stored | Lessons skipped |".to_owned(),
        "|---|---:|---:|---:|---:|---:|".to_owned(),
    ];
    let names: std::collections::BTreeSet<&String> = recall.keys().chain(update.keys()).collect();
    let empty = Value::Null;
    for name in names {
        let r = recall.get(name).unwrap_or(&empty);
        let u = update.get(name).unwrap_or(&empty);
        lines.push(format!(
            "| {name} | {} | {} | {} | {} | {} |",
            int(r, "requests_with_recall_tags"),
            int(r, "injected") + int(r, "delivered_out_of_band"),
            int(u, "tags_found"),
            int(u, "lessons_written"),
            int(u, "lessons_skipped"),
        ));
    }
    lines
}

pub(super) fn render_memory_grammar(data: &Value) -> Vec<String> {
    if data.get("memory_update_tags").is_none() {
        return Vec::new();
    }
    let totals = memory_grammar_totals(data);
    let mut lines = vec![
        String::new(),
        "## Memory verb grammar".to_owned(),
        String::new(),
        concat!(
            "Sessions are taught two tags in the session-start topic catalog: `<mem:recall topic=\"X\"/>` pulls the ",
            "project lessons on a topic into the next request, and `<mem:update topic=\"X\" kind=\"...\">rule</mem:update>` ",
            "records a lesson without a tool call. A tag counts only when a finished reply or request carried it."
        )
        .to_owned(),
        String::new(),
        format!(
            "- `<mem:recall>` tags: {} requests carried one; {} led to lessons being delivered.",
            totals.recall_tag_requests, totals.recall_injected
        ),
        format!(
            "- `<mem:update>` tags: {} found in replies; {} stored as lessons and {} skipped (a duplicate of an existing lesson, or a failed write).",
            totals.update_tags_found, totals.update_lessons_written, totals.update_lessons_skipped
        ),
        format!(
            "- Lessons with source `harvester` created this window: {}.",
            totals.harvested_lessons
        ),
    ];
    lines.extend(grammar_table(data));
    lines
}

pub(super) fn live_skill_status(row: &Value) -> String {
    if super::rows::truthy(row.get("retired_at")) {
        format!("retired ({})", text(row, "retired_reason"))
    } else {
        "shipped".to_owned()
    }
}

pub(super) fn render_skill_live(data: &Value) -> Vec<String> {
    let mut lines = vec![
        String::new(),
        "## Live use of shipped skills".to_owned(),
        String::new(),
    ];
    let live = match data.get("skill_live") {
        None | Some(Value::Null) => {
            lines.push("Not available: the proxy has not applied the migration that adds `memory.skill_live_outcomes`.".to_owned());
            return lines;
        }
        Some(Value::Array(live)) if live.is_empty() => {
            lines.push("No shipped skill has a live measurement yet. The promoter records one on each run.".to_owned());
            return lines;
        }
        Some(Value::Array(live)) => live,
        Some(_) => return lines,
    };
    lines.extend([
        concat!(
            "Loads are Skill tool uses of the shipped folder in its project, counted from the day it shipped (at most 30 days back). ",
            "The corrected rate is the share of the judged operator turns that loaded the skill which the judge marked corrected; ",
            "the base rate is the same share over all of the project's judged turns in the same window. ",
            "The promoter retires a skill with at least 10 loads whose corrected rate is above the base rate."
        )
        .to_owned(),
        String::new(),
        "| Project | Skill | Shipped | Loads | Corrected after a load | Base corrected rate | Status |".to_owned(),
        "|---|---|---|---:|---|---|---|".to_owned(),
    ]);
    for row in live {
        lines.push(format!(
            "| {} | `{}` | {} | {} | {} | {} | {} |",
            text(row, "project"),
            text(row, "folder"),
            text(row, "first_shipped_on"),
            text(row, "loads"),
            rate_text(int(row, "corrected_turns"), int(row, "judged_turns")),
            rate_text(
                int(row, "base_corrected_turns"),
                int(row, "base_judged_turns")
            ),
            live_skill_status(row),
        ));
    }
    lines
}
