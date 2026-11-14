use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::llm::decisions::env::{DecisionEnv, head_chars};
use crate::llm::decisions::proposal_queue::{self, Candidate};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::cross_session::ProposalRow;

const LABEL: &str = "directive proposal";
const SOURCE: &str = "operator_directive_decision";
const OPEN: &str = "open";
const TEXT_CHARS: usize = 600;

fn key_for(id: i64) -> String {
    format!("directive:{id}")
}

fn item(row: &ProposalRow) -> Value {
    json!({
        "id": row.id,
        "directive_kind": row.proposed_kind,
        "directive_text": head_chars(&row.proposed_text, TEXT_CHARS),
        "class": row.class,
        "polarity": row.polarity,
        "scope": row.scope,
        "source": row.source,
        "evidence_count": row.evidence_count,
        "session_count": row.session_count,
        "window_days": row.window_days,
    })
}

fn candidate(row: &ProposalRow) -> Candidate {
    Candidate {
        key: key_for(row.id),
        question_key: format!("directive_{}", row.id),
        label: LABEL,
        item: item(row),
    }
}

pub fn spawn_fill(env: Option<DecisionEnv>, rows: &[ProposalRow]) {
    let open = rows
        .iter()
        .filter(|row| row.status == OPEN)
        .map(candidate)
        .collect();
    proposal_queue::spawn_fill(env, open);
}

pub fn cached(rows: &[ProposalRow]) -> BTreeMap<i64, f64> {
    let keys: Vec<String> = rows.iter().map(|row| key_for(row.id)).collect();
    let found = proposal_queue::cached(keys.iter().map(String::as_str));
    rows.iter()
        .filter_map(|row| found.get(&key_for(row.id)).map(|a| (row.id, *a)))
        .collect()
}

pub fn order_by_suggestion(rows: &mut [ProposalRow], suggested: &BTreeMap<i64, f64>) {
    proposal_queue::order_by_suggestion(rows, suggested, |row| (row.status != OPEN, row.id));
}

pub async fn record_decision(persist: &PersistHandle, id: i64, accepted: bool) {
    proposal_queue::record_operator_decision(
        persist,
        &key_for(id),
        SOURCE,
        json!({ "directive_proposal_id": id }),
        accepted,
    )
    .await;
}

#[cfg(test)]
#[path = "cross_session_order_tests.rs"]
mod tests;
