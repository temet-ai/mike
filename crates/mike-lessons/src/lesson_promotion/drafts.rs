use std::collections::BTreeMap;
use std::path::Path;

use super::model::{DraftRow, LessonRow, skill_key};

#[derive(Debug, Clone)]
pub(super) struct TopicCandidate {
    pub project: String,
    pub folder: String,
    pub rows: Vec<LessonRow>,
    pub roots: Vec<String>,
}

#[derive(Debug, Default)]
pub(super) struct DraftSets {
    pub accepted: BTreeMap<String, DraftRow>,
    pub approved: BTreeMap<String, DraftRow>,
    pub rejected: BTreeMap<String, DraftRow>,
}

impl DraftSets {
    pub(super) fn from_rows(rows: Vec<DraftRow>) -> Self {
        let mut sets = Self::default();
        for row in rows {
            let key = skill_key(&row.project_uuid, &row.folder);
            let target = match row.review.as_deref() {
                Some("approved") => &mut sets.approved,
                Some("rejected") => &mut sets.rejected,
                _ => &mut sets.accepted,
            };
            target.entry(key).or_insert(row);
        }
        sets
    }
}

fn rank(draft: &DraftRow) -> (i64, i32) {
    (draft.lesson_watermark, draft.revision)
}

pub(super) fn chosen_draft<'a>(
    auto: bool,
    accepted: Option<&'a DraftRow>,
    approved: Option<&'a DraftRow>,
    rejected: Option<&DraftRow>,
) -> Option<&'a DraftRow> {
    if !auto {
        return approved;
    }
    let accepted = accepted.filter(|shipped| rejected.is_none_or(|no| rank(no) < rank(shipped)));
    [approved, accepted]
        .into_iter()
        .flatten()
        .reduce(|best, next| if rank(next) > rank(best) { next } else { best })
}

pub(super) fn feeds_topic_skill(row: &LessonRow, topic_slug: &str) -> bool {
    match row.placement.as_deref() {
        None | Some("skill") => true,
        Some("nowhere") => {
            let covered = Path::new(row.covered_by.as_deref().unwrap_or("").trim());
            covered.file_name().is_some_and(|name| name == "SKILL.md")
                && covered
                    .parent()
                    .and_then(Path::file_name)
                    .is_some_and(|parent| parent.to_string_lossy().starts_with(topic_slug))
        }
        Some(_) => false,
    }
}

fn is_reference_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.split('-').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

fn decoded(value: &serde_json::Value) -> Vec<serde_json::Value> {
    match value {
        serde_json::Value::Array(items) => items.clone(),
        serde_json::Value::String(text) => match serde_json::from_str(text) {
            Ok(serde_json::Value::Array(items)) => items,
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

pub(super) fn drafted_references(draft: &DraftRow) -> Vec<(String, String)> {
    decoded(&draft.reference_files)
        .iter()
        .filter_map(|reference| {
            let name = reference.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let content = reference.get("content").and_then(|v| v.as_str())?;
            (name != "lessons" && is_reference_name(name))
                .then(|| (name.to_owned(), content.to_owned()))
        })
        .collect()
}

pub(super) fn keep_reviewed(
    project_uuid: &str,
    lessons: &[LessonRow],
    drafts: &DraftSets,
    candidates: &mut BTreeMap<String, TopicCandidate>,
    template: &TopicCandidate,
) {
    let by_id: BTreeMap<i64, &LessonRow> = lessons.iter().map(|row| (row.id, row)).collect();
    let keys: std::collections::BTreeSet<&String> = drafts
        .accepted
        .keys()
        .chain(drafts.approved.keys())
        .collect();
    for key in keys {
        let (owner, folder) = key.split_once(':').unwrap_or((key.as_str(), ""));
        if owner != project_uuid {
            continue;
        }
        let Some(draft) = drafts
            .approved
            .get(key)
            .or_else(|| drafts.accepted.get(key))
        else {
            continue;
        };
        let mut rows = candidates
            .get(key)
            .map(|current| current.rows.clone())
            .unwrap_or_default();
        let seen: Vec<i64> = rows.iter().map(|row| row.id).collect();
        for id in &draft.lesson_ids {
            if !seen.contains(id)
                && let Some(row) = by_id.get(id)
            {
                rows.push((*row).clone());
            }
        }
        if rows.is_empty() {
            continue;
        }
        rows.sort_by_key(|row| row.id);
        candidates.insert(
            key.clone(),
            TopicCandidate {
                folder: folder.to_owned(),
                rows,
                ..template.clone()
            },
        );
    }
}

#[cfg(test)]
#[path = "drafts_tests.rs"]
mod drafts_tests;
