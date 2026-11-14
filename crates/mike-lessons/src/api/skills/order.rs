use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::state::{Mode, needs_you};
use crate::llm::decisions::env::{DecisionEnv, head_chars};
use crate::llm::decisions::proposal_queue::{self, Candidate};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::lessons::skill_review::SkillFolderRow;
use crate::store::persistence::lessons::skill_review_writes::SkillReview;

const LABEL: &str = "skill draft";
const SOURCE: &str = "operator_skill_review";
const DESCRIPTION_CHARS: usize = 600;

fn key_for(draft_id: i64) -> String {
    format!("skill_draft:{draft_id}")
}

fn item(draft_id: i64, row: &SkillFolderRow) -> Value {
    json!({
        "id": draft_id,
        "skill_folder": row.folder,
        "description": row.description.as_deref().map(|d| head_chars(d, DESCRIPTION_CHARS)),
        "revision": row.revision,
        "lessons_in_draft": row.lesson_count,
        "trigger_score": row.trigger_score,
        "trigger_score_after_patch": row.patched_trigger_score,
        "reviewer": row.reviewer,
        "verdict_reasons": row.verdict_reasons,
        "first_written": row.first_written,
        "live": row.live,
    })
}

fn waiting(rows: &[SkillFolderRow], mode: Mode) -> Vec<(i64, &SkillFolderRow)> {
    rows.iter()
        .filter(|row| needs_you(row, mode))
        .filter_map(|row| Some((row.draft_id?, row)))
        .collect()
}

fn candidate(draft_id: i64, row: &SkillFolderRow) -> Candidate {
    Candidate {
        key: key_for(draft_id),
        question_key: format!("skill_draft_{draft_id}"),
        label: LABEL,
        item: item(draft_id, row),
    }
}

pub fn spawn_fill(env: Option<DecisionEnv>, rows: &[SkillFolderRow], mode: Mode) {
    let candidates = waiting(rows, mode)
        .into_iter()
        .map(|(id, row)| candidate(id, row))
        .collect();
    proposal_queue::spawn_fill(env, candidates);
}

pub fn cached(rows: &[SkillFolderRow], mode: Mode) -> BTreeMap<i64, f64> {
    let waiting = waiting(rows, mode);
    let keys: Vec<String> = waiting.iter().map(|(id, _)| key_for(*id)).collect();
    let found = proposal_queue::cached(keys.iter().map(String::as_str));
    waiting
        .iter()
        .filter_map(|(id, _)| found.get(&key_for(*id)).map(|a| (*id, *a)))
        .collect()
}

pub fn order_by_suggestion(
    rows: &mut [SkillFolderRow],
    suggested: &BTreeMap<i64, f64>,
    mode: Mode,
) {
    proposal_queue::order_by_suggestion(rows, suggested, |row| {
        (!needs_you(row, mode), row.draft_id.unwrap_or(0))
    });
}

pub async fn record_review(persist: &PersistHandle, draft_id: i64, review: SkillReview) {
    let accepted = match review {
        SkillReview::Approved => true,
        SkillReview::Rejected => false,
        SkillReview::Cleared => return,
    };
    proposal_queue::record_operator_decision(
        persist,
        &key_for(draft_id),
        SOURCE,
        json!({ "skill_draft_id": draft_id }),
        accepted,
    )
    .await;
}

#[cfg(test)]
#[path = "order_tests.rs"]
mod tests;
