pub mod compose;
pub mod questions;
pub mod state;

#[cfg(test)]
mod tests;

use serde_json::json;

use crate::dossier::steering_facts::SteeringFacts;
use crate::llm::decisions::point::{Asked, Decider};
use crate::llm::decisions::record::DecisionSubject;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::steering::promotion::DueWakeRow;
pub use compose::{NEEDED_BELOW, Recheck, compose};
pub use questions::{POINT, QUESTION_SET};

pub struct Checked {
    pub recheck: Recheck,
    pub asked: Asked,
}

pub async fn check(
    decider: &Decider,
    persist: &PersistHandle,
    row: &DueWakeRow,
) -> Option<Checked> {
    let facts = SteeringFacts::as_of(
        persist,
        row.session_id,
        row.evaluated_turn_id,
        chrono::Utc::now(),
    )
    .await
    .ok();
    let input = state::WakeInput {
        nudge: &row.payload_text,
        question: row.question.as_deref(),
        facts: facts.as_ref(),
    };
    let asked = decider
        .ask(
            QUESTION_SET,
            state::state(&input),
            questions::questions(),
            DecisionSubject {
                session_id: Some(row.session_id),
                operator_turn_id: Some(row.evaluated_turn_id),
                intervention_uuid: Some(row.uuid),
                ..DecisionSubject::default()
            },
            json!({ "needed_below": NEEDED_BELOW }),
        )
        .await;
    match compose(&asked) {
        Some(recheck) if decider.applies() => Some(Checked { recheck, asked }),
        other => {
            let score = other.map_or(0.0, |r| r.still_needed);
            decider
                .mark(&asked, asked.composed(json!({ "score": score })), "none")
                .await;
            None
        }
    }
}

pub async fn mark(decider: &Decider, checked: &Checked, pushed: bool) {
    let applied = if checked.recheck.skip {
        "held"
    } else if pushed {
        "woken"
    } else {
        "none"
    };
    decider
        .mark(
            &checked.asked,
            checked.asked.composed(checked.recheck.json(pushed)),
            applied,
        )
        .await;
}
