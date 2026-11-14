pub mod questions;

#[cfg(test)]
mod tests;

use serde_json::{Map, Value, json};

pub use questions::{POINT, QUESTION_SET, key};

use crate::llm::decisions::point::{Asked, Decider};
use crate::llm::decisions::record::DecisionSubject;
use crate::llm::decisions::text::{head_chars, tail_chars};
use mike_launch::store::persistence::work_threads::criteria::ThreadCriteria;

pub const ACCEPT_AT: f64 = 0.8;
const ASKS_CHARS: usize = 6_000;
const CRITERIA_CHARS: usize = 900;
const TITLE_CHARS: usize = 160;

pub fn state(threads: &[ThreadCriteria], asks: &str) -> Value {
    let by_key: Map<String, Value> = threads
        .iter()
        .enumerate()
        .map(|(i, t)| {
            (
                key(i),
                json!({
                    "title": head_chars(&t.title, TITLE_CHARS),
                    "slug": t.thread_slug,
                    "criteria": head_chars(&t.criteria_text, CRITERIA_CHARS),
                }),
            )
        })
        .collect();
    json!({
        "operator_asks": tail_chars(asks.trim(), ASKS_CHARS),
        "threads": by_key,
    })
}

pub fn compose(asked: &Asked, count: usize) -> Option<Vec<usize>> {
    if asked.failed() {
        return None;
    }
    let mut accepted = Vec::new();
    for i in 0..count {
        if asked.noul(&key(i))? >= ACCEPT_AT {
            accepted.push(i);
        }
    }
    Some(accepted)
}

pub async fn accepted(
    decider: Option<&Decider>,
    threads: &[ThreadCriteria],
    asks: &str,
    subject: DecisionSubject,
) -> Vec<usize> {
    let Some(decider) = decider else {
        return Vec::new();
    };
    if threads.is_empty() || asks.trim().is_empty() {
        return Vec::new();
    }
    let asked = decider
        .ask(
            QUESTION_SET,
            state(threads, asks),
            questions::questions(threads.len()),
            subject,
            json!({ "accept_at": ACCEPT_AT }),
        )
        .await;
    let found = compose(&asked, threads.len());
    let used = found
        .clone()
        .filter(|_| decider.applies())
        .unwrap_or_default();
    let applied = match (&found, used.is_empty()) {
        (None, _) => "none",
        (Some(_), false) => "chosen",
        (Some(_), true) => "skipped",
    };
    let score = found
        .as_ref()
        .map_or(0.0, |f| f.len() as f64 / threads.len() as f64);
    decider
        .mark(
            &asked,
            asked.composed(json!({ "score": score, "accepted": found })),
            applied,
        )
        .await;
    used
}
