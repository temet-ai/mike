pub mod compose;
pub mod questions;
pub mod state;

#[cfg(test)]
mod tests;

use serde_json::json;
use uuid::Uuid;

use crate::llm::decisions::point::{Asked, Decider};
use crate::llm::decisions::record::DecisionSubject;
pub use compose::{HOLD_BELOW, Verdict, compose};
pub use questions::{POINT, QUESTION_SET};
pub use state::FireInput;

pub struct Subject {
    pub session_db_id: Option<i64>,
    pub operator_turn_id: Option<i64>,
    pub request_id: Option<i64>,
    pub evaluation: Option<Uuid>,
}

pub struct Checked {
    pub verdict: Verdict,
    pub asked: Asked,
}

pub async fn check(decider: &Decider, input: &FireInput<'_>, subject: &Subject) -> Option<Checked> {
    let asked = decider
        .ask(
            QUESTION_SET,
            state::state(input),
            questions::questions(),
            DecisionSubject {
                session_id: subject.session_db_id,
                operator_turn_id: subject.operator_turn_id,
                request_id: subject.request_id,
                evaluation_uuid: subject.evaluation,
                ..DecisionSubject::default()
            },
            json!({ "hold_below": HOLD_BELOW }),
        )
        .await;
    match compose(&asked) {
        Some(verdict) if decider.applies() => Some(Checked { verdict, asked }),
        other => {
            let score = other.map_or(0.0, |v| v.score);
            decider
                .mark(&asked, asked.composed(json!({ "score": score })), "none")
                .await;
            None
        }
    }
}

pub async fn mark(decider: &Decider, checked: &Checked, delivered: bool) {
    let applied = if !checked.verdict.send {
        "held"
    } else if delivered {
        "sent"
    } else {
        "none"
    };
    decider
        .mark(
            &checked.asked,
            checked.asked.composed(checked.verdict.json()),
            applied,
        )
        .await;
}
