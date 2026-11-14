use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::llm::decisions::Question;
use crate::llm::decisions::env::{DecisionEnv, head_chars};
use crate::llm::decisions::point::Asked;
use crate::llm::decisions::proposal_queue::{self, Candidate};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::lessons::placement_proposals::PlacementProposalRow;

pub use crate::llm::decisions::proposal_queue::{POINT, QUESTION_SET, label_for};

const TEXT_CHARS: usize = 600;
const LABEL: &str = "proposal";
const SOURCE: &str = "operator_proposal_decision";

pub fn key_for(lesson_id: i64) -> String {
    format!("lesson:{lesson_id}")
}

fn question_key(lesson_id: i64) -> String {
    format!("proposal_{lesson_id}")
}

fn item(row: &PlacementProposalRow) -> Value {
    json!({
        "id": row.lesson_id,
        "proposed_home": row.placement,
        "lesson_kind": row.kind,
        "lesson_text": head_chars(&row.text, TEXT_CHARS),
        "topic": row.topic,
        "covered_by_document": row.covered_by,
        "reason": row.reason,
        "agent": row.agent_name,
    })
}

fn candidate(row: &PlacementProposalRow) -> Candidate {
    Candidate {
        key: key_for(row.lesson_id),
        question_key: question_key(row.lesson_id),
        label: LABEL,
        item: item(row),
    }
}

fn candidates(group: &[&PlacementProposalRow]) -> Vec<Candidate> {
    group.iter().map(|row| candidate(row)).collect()
}

pub fn questions(group: &[&PlacementProposalRow]) -> BTreeMap<String, Question> {
    let built = candidates(group);
    proposal_queue::questions(&built.iter().collect::<Vec<_>>())
}

pub fn state(group: &[&PlacementProposalRow]) -> Value {
    let built = candidates(group);
    proposal_queue::state(&built.iter().collect::<Vec<_>>())
}

pub fn compose(asked: &Asked, group: &[&PlacementProposalRow]) -> Vec<(i64, f64, String)> {
    let built = candidates(group);
    proposal_queue::compose(asked, &built.iter().collect::<Vec<_>>())
        .into_iter()
        .filter_map(|(key, accept, top)| {
            let id = key.strip_prefix("lesson:")?.parse().ok()?;
            Some((id, accept, top))
        })
        .collect()
}

pub fn cached(rows: &[PlacementProposalRow]) -> BTreeMap<i64, f64> {
    let keys: Vec<String> = rows.iter().map(|r| key_for(r.lesson_id)).collect();
    let found = proposal_queue::cached(keys.iter().map(String::as_str));
    rows.iter()
        .filter_map(|r| found.get(&key_for(r.lesson_id)).map(|a| (r.lesson_id, *a)))
        .collect()
}

pub fn spawn_fill(env: Option<DecisionEnv>, rows: &[PlacementProposalRow]) {
    let undecided = rows
        .iter()
        .filter(|r| r.decision.is_none())
        .map(candidate)
        .collect();
    proposal_queue::spawn_fill(env, undecided);
}

pub async fn record_operator_decision(persist: &PersistHandle, lesson_id: i64, accepted: bool) {
    proposal_queue::record_operator_decision(
        persist,
        &key_for(lesson_id),
        SOURCE,
        json!({ "lesson_id": lesson_id }),
        accepted,
    )
    .await;
}

#[cfg(test)]
#[path = "proposal_order_tests.rs"]
mod tests;
