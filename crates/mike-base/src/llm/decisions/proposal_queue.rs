use std::collections::BTreeMap;
use std::sync::LazyLock;

use dashmap::{DashMap, DashSet};
use serde_json::{Value, json};

use super::env::{DecisionEnv, Pace};
use super::point::{Asked, Decider, choice, record_outcome};
use super::record::DecisionSubject;
use super::{Answer, Question};
use crate::store::persistence::PersistHandle;

pub const POINT: &str = "proposal_order";
pub const QUESTION_SET: &str = "proposal_order_v1";
const GROUP: usize = 5;
const MAX_CALLS: usize = 4;
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

#[derive(Debug, Clone)]
pub struct Candidate {
    pub key: String,
    pub question_key: String,
    pub label: &'static str,
    pub item: Value,
}

static GUESSES: LazyLock<DashMap<String, Guess>> = LazyLock::new(DashMap::new);
static FILLING: LazyLock<DashSet<String>> = LazyLock::new(DashSet::new);

pub fn questions(group: &[&Candidate]) -> BTreeMap<String, Question> {
    group
        .iter()
        .map(|c| {
            (
                c.question_key.clone(),
                choice(
                    &format!(
                        "Will the operator accept, reject or call a duplicate {} {}?",
                        c.label,
                        c.item.get("id").map(Value::to_string).unwrap_or_default()
                    ),
                    OUTCOMES,
                ),
            )
        })
        .collect()
}

pub fn state(group: &[&Candidate]) -> Value {
    json!({ "proposals": group.iter().map(|c| c.item.clone()).collect::<Vec<_>>() })
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

pub fn compose(asked: &Asked, group: &[&Candidate]) -> Vec<(String, f64, String)> {
    if asked.failed() {
        return Vec::new();
    }
    group
        .iter()
        .filter_map(|c| {
            let answer = asked.answers.get(&c.question_key)?;
            let (accept, top) = accept_probability(answer)?;
            Some((c.key.clone(), accept, top))
        })
        .collect()
}

pub fn cached<'a>(keys: impl IntoIterator<Item = &'a str>) -> BTreeMap<String, f64> {
    keys.into_iter()
        .filter_map(|key| GUESSES.get(key).map(|g| (key.to_owned(), g.accept)))
        .collect()
}

pub fn spawn_fill(env: Option<DecisionEnv>, candidates: Vec<Candidate>) {
    let Some(decider) = env.and_then(|env| env.decider(POINT)) else {
        return;
    };
    let fresh: Vec<Candidate> = candidates
        .into_iter()
        .filter(|c| !GUESSES.contains_key(&c.key) && FILLING.insert(c.key.clone()))
        .take(GROUP * MAX_CALLS)
        .collect();
    if fresh.is_empty() {
        return;
    }
    tokio::spawn(async move {
        fill(&decider, &fresh).await;
        for c in &fresh {
            FILLING.remove(&c.key);
        }
    });
}

fn confidence_score(guesses: &[(String, f64, String)]) -> f64 {
    if guesses.is_empty() {
        return 0.0;
    }
    guesses.iter().map(|(_, a, _)| a.max(1.0 - a)).sum::<f64>() / guesses.len() as f64
}

async fn fill(decider: &Decider, candidates: &[Candidate]) {
    let all: Vec<&Candidate> = candidates.iter().collect();
    let mut pace = Pace::new(MAX_CALLS);
    for group in all.chunks(GROUP) {
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
        let applied = if guesses.is_empty() {
            "none"
        } else {
            "ordered"
        };
        let composed = asked.composed(json!({ "score": confidence_score(&guesses) }));
        decider.mark(&asked, composed, applied).await;
        for (key, accept, top) in guesses {
            GUESSES.insert(
                key,
                Guess {
                    accept,
                    top,
                    record_id: asked.record_id,
                },
            );
        }
    }
}

pub fn order_by_suggestion<T>(
    rows: &mut [T],
    suggested: &BTreeMap<i64, f64>,
    decided_and_id: impl Fn(&T) -> (bool, i64),
) {
    rows.sort_by(|a, b| {
        let key = |row: &T| {
            let (decided, id) = decided_and_id(row);
            (decided, suggested.get(&id).map(|p| -p))
        };
        let (da, pa) = key(a);
        let (db, pb) = key(b);
        da.cmp(&db).then_with(|| match (pa, pb) {
            (Some(x), Some(y)) => x.total_cmp(&y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        })
    });
}

pub fn label_for(top: &str, accepted: bool) -> &'static str {
    if (top == ACCEPT) == accepted {
        "right"
    } else {
        "wrong"
    }
}

pub async fn record_operator_decision(
    persist: &PersistHandle,
    key: &str,
    source: &str,
    mut evidence: Value,
    accepted: bool,
) {
    let Some((_, guess)) = GUESSES.remove(key) else {
        return;
    };
    let Some(record_id) = guess.record_id else {
        return;
    };
    if let Some(fields) = evidence.as_object_mut() {
        fields.insert("accepted".to_owned(), json!(accepted));
        fields.insert("predicted".to_owned(), json!(guess.top));
    }
    record_outcome(
        persist,
        record_id,
        label_for(&guess.top, accepted),
        source,
        evidence,
    )
    .await;
}

#[cfg(test)]
#[path = "proposal_queue_tests.rs"]
mod tests;
