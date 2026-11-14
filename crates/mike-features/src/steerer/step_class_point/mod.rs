pub mod compose;
pub mod questions;

#[cfg(test)]
mod tests;

use serde_json::{Value, json};

pub use compose::{CHOOSE_AT, compose};
pub use questions::{POINT, QUESTION_SET};

use crate::llm::decisions::point::Decider;
use crate::llm::decisions::record::DecisionSubject;
use crate::llm::decisions::text::{head_chars, tail_chars};
use crate::steerer::guard::step_class::{StepClass, asked_sentence};

const SENTENCE_CHARS: usize = 400;
const ENDING_CHARS: usize = 600;

pub fn state(sentence: &str, ending: &str) -> Value {
    json!({
        "sentence": head_chars(sentence, SENTENCE_CHARS),
        "turn_end": tail_chars(ending.trim_end(), ENDING_CHARS),
    })
}

pub async fn pick(decider: &Decider, ending: &str, subject: DecisionSubject) -> Option<StepClass> {
    let sentence = asked_sentence(ending)?;
    let asked = decider
        .ask(
            QUESTION_SET,
            state(&sentence, ending),
            questions::questions(),
            subject,
            json!({ "choose_at": CHOOSE_AT }),
        )
        .await;
    let chosen = compose(&asked);
    let score = chosen.map_or(0.0, |(_, p)| p);
    let used = chosen.filter(|_| decider.applies());
    let applied = if used.is_some() { "chosen" } else { "none" };
    decider
        .mark(&asked, asked.composed(json!({ "score": score })), applied)
        .await;
    used.map(|(class, _)| class)
}
