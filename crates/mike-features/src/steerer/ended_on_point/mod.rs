pub mod compose;
pub mod questions;

#[cfg(test)]
mod tests;

use serde_json::json;

pub use compose::{DONE_AT, ENDS_ON_AT, Ending, Reading, apply, compose};
pub use questions::{POINT, QUESTION_SET};

use crate::llm::decisions::point::Decider;
use crate::llm::decisions::record::DecisionSubject;
use crate::llm::decisions::text::tail_chars;
use crate::steerer::guard::gates::PhraseGates;

const ENDING_CHARS: usize = 1_500;

pub fn state(ending: &str) -> serde_json::Value {
    json!({ "turn_end": tail_chars(ending.trim_end(), ENDING_CHARS) })
}

pub async fn refine(
    decider: &Decider,
    phrase: PhraseGates,
    ending: &str,
    subject: DecisionSubject,
) -> PhraseGates {
    let asked = decider
        .ask(
            QUESTION_SET,
            state(ending),
            questions::questions(),
            subject,
            json!({ "ends_on_at": ENDS_ON_AT, "done_at": DONE_AT }),
        )
        .await;
    let reading = compose(&asked);
    let score = reading.as_ref().map_or(0.0, Reading::score);
    let applying = reading.filter(|_| decider.applies());
    let applied = if applying.is_some() { "chosen" } else { "none" };
    decider
        .mark(&asked, asked.composed(json!({ "score": score })), applied)
        .await;
    match applying {
        Some(reading) => apply(phrase, &reading),
        None => phrase,
    }
}
