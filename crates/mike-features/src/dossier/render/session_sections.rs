use std::collections::HashMap;

use super::cut::cut;
use super::session_turn::collapse;
use crate::store::persistence::dossier_entries::DossierEntryRow;
use crate::store::persistence::dossier_view::{RecapRow, SuppliedLessonRow};

const RECAP_CAP: usize = 800;
const LESSON_CAP: usize = 400;
const ENTRY_CAP: usize = 400;

pub const RECAPS_HEADING: &str =
    "RECAPS (Claude's own account while the operator was away; it may be wrong):";
pub const LESSONS_HEADING: &str = "LESSONS SUPPLIED (newest 20, each lesson once):";
pub const PROMISES_HEADING: &str = "PROMISES (open or broken as of now):";
pub const DIRECTIVES_HEADING: &str = "DIRECTIVES IN FORCE:";

pub struct Numbers(HashMap<i64, usize>);

impl Numbers {
    pub fn new(turn_ids: impl Iterator<Item = i64>) -> Self {
        Self(
            turn_ids
                .enumerate()
                .map(|(index, id)| (id, index + 1))
                .collect(),
        )
    }

    pub fn label(&self, turn_id: i64) -> String {
        self.0
            .get(&turn_id)
            .map_or_else(|| format!("turn id {turn_id}"), |n| format!("#{n}"))
    }
}

pub fn section(heading: &str, lines: Vec<String>) -> String {
    if lines.is_empty() {
        format!("{heading}\n(none)")
    } else {
        format!("{heading}\n{}", lines.join("\n"))
    }
}

pub fn recap_lines(recaps: &[RecapRow]) -> Vec<String> {
    recaps
        .iter()
        .map(|r| {
            format!(
                "- {}: {}",
                r.at.format("%Y-%m-%d %H:%M"),
                cut(&collapse(&r.text), RECAP_CAP)
            )
        })
        .collect()
}

pub fn lesson_lines(lessons: &[SuppliedLessonRow], numbers: &Numbers) -> Vec<String> {
    lessons
        .iter()
        .map(|l| {
            let on = l.turn_id.map_or_else(
                || format!("request {}", l.request_id),
                |id| numbers.label(id),
            );
            format!(
                "- lesson {} via {} on {on}: {}",
                l.lesson_id,
                l.route,
                cut(&collapse(&l.text), LESSON_CAP)
            )
        })
        .collect()
}

pub fn promise_lines(entries: &[DossierEntryRow], numbers: &Numbers) -> Vec<String> {
    entries
        .iter()
        .filter(|e| e.kind == "promise" && matches!(e.status.as_str(), "open" | "broken"))
        .map(|e| {
            let on = e
                .operator_turn_id
                .map_or_else(|| "no turn".to_owned(), |id| numbers.label(id));
            format!(
                "- {on} {}: {}",
                e.status,
                cut(&collapse(&e.data.to_string()), ENTRY_CAP)
            )
        })
        .collect()
}

pub fn directive_lines(entries: &[DossierEntryRow]) -> Vec<String> {
    let field = |e: &DossierEntryRow, key: &str| {
        e.data
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_owned()
    };
    entries
        .iter()
        .filter(|e| e.kind == "directive" && e.status == "open")
        .map(|e| {
            format!(
                "- {} {} ({})",
                field(e, "class"),
                field(e, "polarity"),
                e.source_ref
            )
        })
        .collect()
}
