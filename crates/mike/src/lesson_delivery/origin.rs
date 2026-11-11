use serde_json::json;

use super::LESSONS_HEADING;
use super::held::{find, tag_at};
use crate::compress::manifest::{ManifestBuilder, UnchangedStage};

pub(super) const MOD_LESSONS_STAGE: &str = "mod_lessons";
pub(super) const MOD_LESSONS_REASON: &str = "lessons_added_by_mod";
pub(super) const TETHER_LESSON_PREFIX: &str =
    "Lesson recorded from earlier sessions in this project ";

pub(super) fn mod_lesson_ids(body: &[u8]) -> Vec<i64> {
    let mut ids = heading_block_ids(body);
    for id in tether_line_ids(body) {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

pub(super) fn tether_line_ids(body: &[u8]) -> Vec<i64> {
    let prefix = TETHER_LESSON_PREFIX.as_bytes();
    let mut ids = Vec::new();
    let mut from = 0;
    while let Some(offset) = find(body.get(from..).unwrap_or_default(), prefix) {
        let tag_start = from + offset + prefix.len();
        from = tag_start;
        if let Some((id, _)) = tag_at(body, tag_start)
            && !ids.contains(&id)
        {
            ids.push(id);
        }
    }
    ids
}

pub(super) fn heading_block_ids(body: &[u8]) -> Vec<i64> {
    let heading = LESSONS_HEADING.as_bytes();
    let mut ids = Vec::new();
    let mut from = 0;
    while let Some(offset) = find(body.get(from..).unwrap_or_default(), heading) {
        let after_heading = from + offset + heading.len();
        from = after_heading;
        let mut line = line_start_at(body, after_heading);
        while let Some(start) = line {
            let Some((id, end)) = tag_at(body, start) else {
                break;
            };
            if !ids.contains(&id) {
                ids.push(id);
            }
            line = next_line_start(body, end);
        }
    }
    ids
}

fn line_start_at(body: &[u8], at: usize) -> Option<usize> {
    match (body.get(at), body.get(at + 1)) {
        (Some(b'\\'), Some(b'n')) => Some(at + 2),
        (Some(b'\n'), _) => Some(at + 1),
        _ => None,
    }
}

fn next_line_start(body: &[u8], from: usize) -> Option<usize> {
    let mut at = from;
    while let Some(byte) = body.get(at) {
        match byte {
            b'\\' => {
                if body.get(at + 1) == Some(&b'n') {
                    return Some(at + 2);
                }
                at += 2;
            }
            b'\n' => return Some(at + 1),
            b'"' => return None,
            _ => at += 1,
        }
    }
    None
}

pub(crate) fn label_mod_lessons(body: &[u8], manifest: &mut ManifestBuilder) -> Vec<i64> {
    let ids = mod_lesson_ids(body);
    record_mod_lessons(manifest, &ids);
    ids
}

fn record_mod_lessons(manifest: &mut ManifestBuilder, ids: &[i64]) {
    if ids.is_empty() {
        return;
    }
    manifest.record_unchanged_with(UnchangedStage {
        stage: MOD_LESSONS_STAGE,
        policy_id: MOD_LESSONS_STAGE,
        reason_code: MOD_LESSONS_REASON,
        resolved_parameters: json!({ "origin": "mod", "lesson_ids": ids }),
        targets: Vec::new(),
        detail: None,
    });
}
