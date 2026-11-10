use serde_json::Value;

use super::best_shape::{fit, one_line};
use super::passage_excerpt::head;
use crate::analysis::rerank::MAX_DOCUMENT_CHARS;

const PART_SEPARATOR: &str = " | ";
const LABEL_SEPARATOR: &str = ", ";
const DATE_CHARS: usize = 10;
const NAME_CHARS: usize = 80;
const TURN_KIND: &str = "turn";

pub(crate) fn header(item: &Value) -> String {
    assemble(label(item), day(item), name(item))
}

pub(crate) fn turn_header(date: &str, session_title: Option<&str>) -> String {
    assemble(
        TURN_KIND.to_owned(),
        Some(date),
        session_title.and_then(name_text),
    )
}

pub(crate) fn with_header(header: &str, body: &str) -> String {
    head(&format!("{header}\n{body}"), MAX_DOCUMENT_CHARS).to_owned()
}

pub(super) fn label(item: &Value) -> String {
    let kind = item["source_kind"].as_str().unwrap_or("item");
    let provenance = &item["provenance"];
    let fields: &[&str] = match kind {
        "note" => &["kind"],
        "lesson" => &["lesson_kind", "topic"],
        _ => &[],
    };
    let extras = fields
        .iter()
        .filter_map(|field| provenance[*field].as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty());
    std::iter::once(kind)
        .chain(extras)
        .collect::<Vec<_>>()
        .join(LABEL_SEPARATOR)
}

pub(super) fn day(item: &Value) -> Option<&str> {
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
    shaped.then_some(day)
}

fn name(item: &Value) -> Option<String> {
    let provenance = &item["provenance"];
    let raw = match item["source_kind"].as_str() {
        Some("work_thread" | "thread_entry") => provenance["slug"].as_str(),
        _ => present(&item["path"]).or_else(|| present(&item["author"]["session_title"])),
    };
    raw.and_then(name_text)
}

fn present(value: &Value) -> Option<&str> {
    value.as_str().filter(|text| !text.trim().is_empty())
}

fn name_text(text: &str) -> Option<String> {
    let line = one_line(text);
    (!line.is_empty()).then(|| fit(&line, NAME_CHARS))
}

fn assemble(label: String, date: Option<&str>, name: Option<String>) -> String {
    [Some(label), date.map(str::to_owned), name]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(PART_SEPARATOR)
}

#[cfg(test)]
#[path = "item_header_tests.rs"]
mod tests;
