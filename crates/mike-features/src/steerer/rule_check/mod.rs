pub mod compose;
pub mod questions;
pub mod rules;
pub mod state;

#[cfg(test)]
mod tests;

use serde_json::json;

use crate::llm::decisions::point::{Asked, Decider};
use crate::llm::decisions::record::DecisionSubject;
pub use compose::{Outcome, VIOLATION_AT, compose};
pub use questions::{POINT, QUESTION_SET};
pub use rules::{Rule, TurnTools};
pub use state::TurnInput;

pub const RULES_PER_TURN: usize = 5;

pub struct Checked<'a> {
    pub outcome: Outcome,
    pub rule: &'a Rule,
    pub asked: Asked,
}

pub async fn check<'a>(
    decider: &Decider,
    input: &TurnInput<'_>,
    picked: &[&'a Rule],
    subject: DecisionSubject,
) -> Option<Checked<'a>> {
    if picked.is_empty() {
        return None;
    }
    let asked = decider
        .ask(
            QUESTION_SET,
            state::state(input, picked),
            questions::questions(picked),
            subject,
            json!({ "violation_at": VIOLATION_AT, "rules_per_turn": RULES_PER_TURN }),
        )
        .await;
    match compose(&asked, picked.len()) {
        Some(outcome) if decider.applies() => {
            let rule = picked.get(outcome.rule).copied()?;
            Some(Checked {
                outcome,
                rule,
                asked,
            })
        }
        other => {
            let score = other.map_or(0.0, |o| o.score());
            decider
                .mark(&asked, asked.composed(json!({ "score": score })), "none")
                .await;
            None
        }
    }
}

pub async fn mark(decider: &Decider, checked: &Checked<'_>, delivered: bool) {
    let applied = if checked.outcome.violated && delivered {
        "warned"
    } else {
        "none"
    };
    decider
        .mark(
            &checked.asked,
            checked.asked.composed(checked.outcome.json(delivered)),
            applied,
        )
        .await;
}
