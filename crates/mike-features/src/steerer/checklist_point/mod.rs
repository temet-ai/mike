pub mod compose;
pub mod facts;
pub mod questions;
pub mod state;

#[cfg(test)]
mod facts_tests;
#[cfg(test)]
mod state_tests;
#[cfg(test)]
mod tests;

use serde_json::{Value, json};

pub use compose::{Checklist, Item, NOT_DONE_AT, Status, compose};
pub use facts::{earlier_turns, ends_with_operator_question};
pub use questions::{POINT, QUESTION_SET};
pub use state::{Criterion, Evidence, MAX_CRITERIA, intake_criteria, is_rule, thread_criteria};

use crate::llm::decisions::point::{Asked, Decider};
use crate::llm::decisions::record::DecisionSubject;

pub struct Checked {
    pub checklist: Checklist,
    pub asked: Asked,
}

fn composed_of(checklist: &Checklist) -> Value {
    json!({
        "score": checklist.confidence(),
        "fires": checklist.fires(),
        "flagged": checklist.flagged().len(),
        "open_stated": checklist.open_stated().len(),
    })
}

pub async fn check(
    decider: &Decider,
    criteria: &[Criterion],
    evidence: &Evidence<'_>,
    subject: DecisionSubject,
) -> Option<Checked> {
    if criteria.is_empty() {
        return None;
    }
    let asked = decider
        .ask(
            QUESTION_SET,
            state::state(criteria, evidence),
            questions::questions(criteria),
            subject,
            json!({ "not_done_at": NOT_DONE_AT }),
        )
        .await;
    let Some(checklist) = compose(&asked, criteria) else {
        decider
            .mark(&asked, asked.composed(json!({ "score": 0.0 })), "none")
            .await;
        return None;
    };
    if !decider.applies() {
        decider
            .mark(&asked, asked.composed(composed_of(&checklist)), "none")
            .await;
        return None;
    }
    Some(Checked { checklist, asked })
}

pub async fn mark(decider: &Decider, checked: &Checked) {
    let applied = if checked.checklist.fires() {
        "chosen"
    } else {
        "skipped"
    };
    decider
        .mark(
            &checked.asked,
            checked.asked.composed(composed_of(&checked.checklist)),
            applied,
        )
        .await;
}

pub async fn label_against_writer(decider: &Decider, checked: &Checked, writer_fires: bool) {
    let fires = checked.checklist.fires();
    let label = if fires == writer_fires {
        "right"
    } else {
        "wrong"
    };
    decider
        .outcome(
            &checked.asked,
            label,
            "steerer_llm_verdict",
            json!({ "checklist_fires": fires, "writer_fires": writer_fires }),
        )
        .await;
}
