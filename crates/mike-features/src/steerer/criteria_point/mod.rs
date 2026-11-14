pub mod compose;
pub mod questions;

#[cfg(test)]
mod tests;

use serde_json::{Map, Value, json};

pub use compose::{ACCEPT_AT, compose, mean_confidence};
pub use questions::{POINT, QUESTION_SET};

use crate::llm::decisions::point::Decider;
use crate::llm::decisions::record::DecisionSubject;
use crate::llm::decisions::text::head_chars;

const CRITERION_CHARS: usize = 500;
const MAX_CRITERIA: usize = 20;

pub fn state(criteria: &[&str]) -> Value {
    let by_key: Map<String, Value> = criteria
        .iter()
        .enumerate()
        .map(|(i, text)| {
            (
                questions::key(i),
                Value::String(head_chars(text.trim(), CRITERION_CHARS).to_owned()),
            )
        })
        .collect();
    json!({ "criteria": by_key })
}

pub async fn judge(
    decider: &Decider,
    criteria: &[&str],
    subject: DecisionSubject,
) -> Option<Vec<bool>> {
    if criteria.is_empty() || criteria.len() > MAX_CRITERIA {
        return None;
    }
    let asked = decider
        .ask(
            QUESTION_SET,
            state(criteria),
            questions::questions(criteria.len()),
            subject,
            json!({ "accept_at": ACCEPT_AT }),
        )
        .await;
    let verdicts = compose(&asked, criteria.len());
    let score = mean_confidence(&asked, criteria.len());
    let used = if decider.applies() { verdicts } else { None };
    let applied = if used.is_some() { "chosen" } else { "none" };
    decider
        .mark(&asked, asked.composed(json!({ "score": score })), applied)
        .await;
    used
}
