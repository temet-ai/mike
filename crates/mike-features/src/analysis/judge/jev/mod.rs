pub mod compose;
pub mod questions;
pub mod state;

#[cfg(test)]
mod tests;

use serde_json::json;

use crate::llm::decisions::point::{Asked, Decider};
use crate::llm::decisions::record::DecisionSubject;
pub use compose::{Composed, UNCERTAIN_BELOW, YES, compose};
pub use questions::{POINT, QUESTION_SET, Shape};
pub use state::TurnFacts;

#[derive(Debug, Clone)]
pub struct JevVerdict {
    pub composed: Composed,
    pub asked: Asked,
}

fn subject(facts: &TurnFacts<'_>) -> DecisionSubject {
    DecisionSubject {
        session_id: facts.session_id,
        operator_turn_id: Some(facts.turn_id),
        ..DecisionSubject::default()
    }
}

pub async fn decide(
    decider: &Decider,
    facts: &TurnFacts<'_>,
    shape: Shape,
    threshold: f32,
) -> Option<JevVerdict> {
    let with_next = matches!(shape, Shape::TwoAxis { has_next: true });
    let thresholds = json!({
        "yes": YES,
        "uncertain_below": UNCERTAIN_BELOW,
        "confidence_threshold": threshold,
    });
    let asked = decider
        .ask(
            QUESTION_SET,
            state::state(facts, with_next),
            questions::questions(shape),
            subject(facts),
            thresholds,
        )
        .await;
    match compose(&asked, shape) {
        Some(composed) if decider.applies() => Some(JevVerdict { composed, asked }),
        other => {
            let score = other.as_ref().map_or(0.0, |c| c.confidence);
            decider
                .mark(&asked, asked.composed(json!({ "score": score })), "none")
                .await;
            None
        }
    }
}

pub async fn mark_stored(decider: &Decider, verdict: &JevVerdict, threshold: f32) {
    decider
        .mark(
            &verdict.asked,
            verdict.asked.composed(verdict.composed.json(threshold)),
            "sent",
        )
        .await;
}
