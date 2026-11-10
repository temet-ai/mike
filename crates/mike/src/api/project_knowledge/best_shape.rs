use serde_json::{Value, json};

use super::best_lean::{lead_item, lean_envelope};
use super::passage_excerpt::{Passages, head, show_passage};
use crate::search_ranking::is_outside_project;

pub(crate) const FIRST_PASSAGE_CHARS: usize = 1_500;
pub(crate) const LINE_CHARS: usize = 120;
const PART_SEPARATOR: &str = " \u{b7} ";
const MACHINE_WRITTEN: &str = " \u{b7} machine-written";
const OUTSIDE_PROJECT: &str = " \u{b7} outside this project";
const CUT_MARK: &str = "...";
const DATE_CHARS: usize = 10;
pub(crate) const HOW_TO_GET_THE_REST: &str =
    "Read one with read_project_knowledge and its reference, or repeat with detail=full.";

pub(crate) fn best_only(answer: &mut Value, passages: &Passages) {
    let has_more = answer["has_more"].as_bool().unwrap_or(false);
    let pointers = shape_items(answer, passages);
    if pointers > 0 || has_more {
        answer["more"] = json!({
            "shown_in_full": 1,
            "pointers": pointers,
            "how": HOW_TO_GET_THE_REST,
        });
    }
    lean_envelope(answer);
}

fn shape_items(answer: &mut Value, passages: &Passages) -> usize {
    let Some((first, rest)) = answer["items"]
        .as_array_mut()
        .and_then(|items| items.split_first_mut())
    else {
        return 0;
    };
    show_passage(first, passages, FIRST_PASSAGE_CHARS);
    *first = lead_item(first);
    for item in rest.iter_mut() {
        *item = pointer(item);
    }
    rest.len()
}

fn pointer(item: &Value) -> Value {
    json!({"reference": item["reference"].clone(), "line": line(item)})
}

pub(crate) fn line(item: &Value) -> String {
    let kind = item["source_kind"].as_str().unwrap_or("item");
    let prefix = format!("{kind}{PART_SEPARATOR}{}{PART_SEPARATOR}", day(item));
    let machine = item["provenance"]["written_by"].as_str() == Some("machine");
    let suffix = format!(
        "{}{}",
        if machine { MACHINE_WRITTEN } else { "" },
        if is_outside_project(item) {
            OUTSIDE_PROJECT
        } else {
            ""
        }
    );
    let used = prefix.chars().count() + suffix.chars().count();
    let title = one_line(item["title"].as_str().unwrap_or("(untitled)"));
    format!(
        "{prefix}{}{suffix}",
        fit(&title, LINE_CHARS.saturating_sub(used))
    )
}

fn day(item: &Value) -> &str {
    let stamp = item["updated_at"].as_str().unwrap_or_default();
    let day = head(stamp, DATE_CHARS);
    let shaped = day.chars().count() == DATE_CHARS
        && day.chars().enumerate().all(|(at, c)| {
            if matches!(at, 4 | 7) {
                c == '-'
            } else {
                c.is_ascii_digit()
            }
        });
    if shaped { day } else { "undated" }
}

pub(super) fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn fit(text: &str, room: usize) -> String {
    if text.chars().count() <= room {
        return text.to_owned();
    }
    match room.checked_sub(CUT_MARK.len()) {
        Some(kept) => format!("{}{CUT_MARK}", head(text, kept)),
        None => head(text, room).to_owned(),
    }
}

#[cfg(test)]
#[path = "best_shape_tests.rs"]
mod tests;
