use serde_json::{Value, json};

use super::best_lean::{later_mark, lead_item, lean_envelope, session_title};
use super::item_header;
use super::passage_excerpt::{Passages, head, show_passage};
use crate::search_ranking::is_outside_project;

pub(crate) const FIRST_PASSAGE_CHARS: usize = 1_500;
pub(crate) const LINE_CHARS: usize = 120;
const PART_SEPARATOR: &str = " \u{b7} ";
const MACHINE_WRITTEN: &str = " \u{b7} machine-written";
const OUTSIDE_PROJECT: &str = " \u{b7} outside this project";
const CUT_MARK: &str = "...";
const SHORT_KEY_CHARS: usize = 8;
const SESSION_TITLE_CHARS: usize = 32;
pub(crate) const HOW_TO_GET_THE_REST: &str =
    "Read one with read_project_knowledge or use detail=full. Cite the reference you use.";

pub(crate) fn best_only(answer: &mut Value, passages: &Passages) {
    let has_more = answer["has_more"].as_bool().unwrap_or(false);
    let has_items = answer["items"]
        .as_array()
        .is_some_and(|items| !items.is_empty());
    let pointers = shape_items(answer, passages);
    if has_items || has_more {
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
    let label = item_header::label(item);
    let day = item_header::day(item).unwrap_or("undated");
    let prefix = format!("{label}{PART_SEPARATOR}{day}{PART_SEPARATOR}");
    let machine = item["provenance"]["written_by"].as_str() == Some("machine");
    let suffix = format!(
        "{}{}{}{}",
        if machine { MACHINE_WRITTEN } else { "" },
        later_suffix(item),
        if is_outside_project(item) {
            OUTSIDE_PROJECT
        } else {
            ""
        },
        session_suffix(item)
    );
    let used = prefix.chars().count() + suffix.chars().count();
    let title = one_line(item["title"].as_str().unwrap_or("(untitled)"));
    format!(
        "{prefix}{}{suffix}",
        fit(&title, LINE_CHARS.saturating_sub(used))
    )
}

fn later_suffix(item: &Value) -> String {
    later_mark(item)
        .map(|(_, words, reference)| {
            let (kind, key) = reference.split_once(':').unwrap_or(("", reference));
            format!(
                "{PART_SEPARATOR}{words} {kind}:{}",
                head(key, SHORT_KEY_CHARS)
            )
        })
        .unwrap_or_default()
}

fn session_suffix(item: &Value) -> String {
    session_title(item)
        .map(|title| {
            format!(
                "{PART_SEPARATOR}by {}",
                fit(&one_line(title), SESSION_TITLE_CHARS)
            )
        })
        .unwrap_or_default()
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
