pub mod compose;
pub mod questions;

#[cfg(test)]
mod tests;

use serde_json::{Value, json};

pub use compose::{PROD_AT, Verdict, compose};
pub use questions::{POINT, QUESTION_SET};

use crate::llm::decisions::point::Decider;
use crate::llm::decisions::record::DecisionSubject;
use crate::llm::decisions::text::head_chars;
use crate::steerer::guard::label_rules::is_prod;

const MESSAGE_CHARS: usize = 600;

pub fn state(message: &str) -> Value {
    json!({
        "message": head_chars(message.trim(), MESSAGE_CHARS),
        "message_chars": message.trim().chars().count(),
    })
}

pub async fn detect(decider: Option<&Decider>, message: &str, subject: DecisionSubject) -> bool {
    let regex_answer = is_prod(message);
    let Some(decider) = decider else {
        return regex_answer;
    };
    if message.trim().is_empty() {
        return regex_answer;
    }
    let asked = decider
        .ask(
            QUESTION_SET,
            state(message),
            questions::questions(),
            subject,
            json!({ "prod_at": PROD_AT }),
        )
        .await;
    let verdict = compose(&asked);
    let used = verdict.filter(|_| decider.applies());
    let composed = json!({
        "score": verdict.map_or(0.0, |v| v.score()),
        "regex_said_prod": regex_answer,
    });
    let applied = if used.is_some() { "chosen" } else { "none" };
    decider
        .mark(&asked, asked.composed(composed), applied)
        .await;
    used.map_or(regex_answer, |v| v.is_prod)
}
