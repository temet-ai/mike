use std::collections::BTreeMap;
use std::sync::LazyLock;

use dashmap::{DashMap, DashSet};
use serde_json::{Value, json};

use crate::llm::decisions::env::{DecisionEnv, Pace, head_chars};
use crate::llm::decisions::point::{Asked, choice, record_outcome};
use crate::llm::decisions::{Answer, Question, record::DecisionSubject};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::lessons::placement_proposals::PlacementProposalRow;

pub const POINT: &str = "proposal_order";
pub const QUESTION_SET: &str = "proposal_order_v1";
const GROUP: usize = 5;
const MAX_CALLS: usize = 4;
const TEXT_CHARS: usize = 600;
const ACCEPT: &str = "accept";

const OUTCOMES: &[(&str, &str)] = &[
    (ACCEPT, "the operator will accept the proposal as written"),
    ("reject", "the operator will reject the proposal"),
    (
        "duplicate",
        "the proposal repeats something the project already has",
    ),
];

#[derive(Debug, Clone, PartialEq)]
pub struct Guess {
    pub accept: f64,
    pub top: String,
    pub record_id: Option<i64>,
}

static GUESSES: LazyLock<DashMap<String, Guess>> = LazyLock::new(DashMap::new);

pub fn key_for(lesson_id: i64) -> String {
    format!("lesson:{lesson_id}")
}

fn question_key(lesson_id: i64) -> String {
    format!("proposal_{lesson_id}")
}

pub fn questions(group: &[&PlacementProposalRow]) -> BTreeMap<String, Question> {
    group
        .iter()
        .map(|row| {
            (
                question_key(row.lesson_id),
                choice(
                    &format!(
                        "Will the operator accept, reject or call a duplicate proposal {}?",
                        row.lesson_id
                    ),
                    OUTCOMES,
                ),
            )
        })
        .collect()
}

pub fn state(group: &[&PlacementProposalRow]) -> Value {
    json!({
        "proposals": group
            .iter()
            .map(|row| json!({
                "id": row.lesson_id,
                "proposed_home": row.placement,
                "lesson_kind": row.kind,
                "lesson_text": head_chars(&row.text, TEXT_CHARS),
                "topic": row.topic,
                "covered_by_document": row.covered_by,
                "reason": row.reason,
                "agent": row.agent_name,
            }))
            .collect::<Vec<_>>(),
    })
}

fn accept_probability(answer: &Answer) -> Option<(f64, String)> {
    let Answer::Choice {
        choice,
        probabilities,
        confidence,
    } = answer
    else {
        return None;
    };
    let top = probabilities.get(choice).copied().or(*confidence)?;
    let accept = probabilities
        .get(ACCEPT)
        .copied()
        .unwrap_or(if choice == ACCEPT { top } else { 0.0 });
    Some((accept, choice.clone()))
}

pub fn compose(asked: &Asked, group: &[&PlacementProposalRow]) -> Vec<(i64, f64, String)> {
    if asked.failed() {
        return Vec::new();
    }
    group
        .iter()
        .filter_map(|row| {
            let answer = asked.answers.get(&question_key(row.lesson_id))?;
            let (accept, top) = accept_probability(answer)?;
            Some((row.lesson_id, accept, top))
        })
        .collect()
}

static FILLING: LazyLock<DashSet<i64>> = LazyLock::new(DashSet::new);

pub fn cached(rows: &[PlacementProposalRow]) -> BTreeMap<i64, f64> {
    rows.iter()
        .filter_map(|r| {
            GUESSES
                .get(&key_for(r.lesson_id))
                .map(|g| (r.lesson_id, g.accept))
        })
        .collect()
}

pub fn spawn_fill(env: Option<DecisionEnv>, rows: &[PlacementProposalRow]) {
    let Some(env) = env else {
        return;
    };
    let fresh: Vec<PlacementProposalRow> = rows
        .iter()
        .filter(|r| {
            r.decision.is_none()
                && !GUESSES.contains_key(&key_for(r.lesson_id))
                && FILLING.insert(r.lesson_id)
        })
        .take(GROUP * MAX_CALLS)
        .cloned()
        .collect();
    if fresh.is_empty() {
        return;
    }
    tokio::spawn(async move {
        fill(&env, &fresh).await;
        for row in &fresh {
            FILLING.remove(&row.lesson_id);
        }
    });
}

async fn fill(env: &DecisionEnv, rows: &[PlacementProposalRow]) {
    let Some(decider) = env.decider(POINT) else {
        return;
    };
    let fresh: Vec<&PlacementProposalRow> = rows.iter().collect();
    let mut pace = Pace::new(MAX_CALLS);
    for group in fresh.chunks(GROUP) {
        if !pace.may_ask() {
            break;
        }
        let asked = decider
            .ask(
                QUESTION_SET,
                state(group),
                questions(group),
                DecisionSubject::default(),
                json!({}),
            )
            .await;
        pace.spent(&asked);
        let guesses = compose(&asked, group);
        let score = if guesses.is_empty() {
            0.0
        } else {
            guesses.iter().map(|(_, a, _)| a.max(1.0 - a)).sum::<f64>() / guesses.len() as f64
        };
        let applied = if guesses.is_empty() {
            "none"
        } else {
            "ordered"
        };
        decider
            .mark(&asked, asked.composed(json!({ "score": score })), applied)
            .await;
        for (id, accept, top) in guesses {
            GUESSES.insert(
                key_for(id),
                Guess {
                    accept,
                    top,
                    record_id: asked.record_id,
                },
            );
        }
    }
}

pub fn label_for(top: &str, accepted: bool) -> &'static str {
    if (top == ACCEPT) == accepted {
        "right"
    } else {
        "wrong"
    }
}

pub async fn record_operator_decision(persist: &PersistHandle, lesson_id: i64, accepted: bool) {
    let Some((_, guess)) = GUESSES.remove(&key_for(lesson_id)) else {
        return;
    };
    let Some(record_id) = guess.record_id else {
        return;
    };
    record_outcome(
        persist,
        record_id,
        label_for(&guess.top, accepted),
        "operator_proposal_decision",
        json!({ "lesson_id": lesson_id, "accepted": accepted, "predicted": guess.top }),
    )
    .await;
}

#[cfg(test)]
#[path = "proposal_order_tests.rs"]
mod tests;
