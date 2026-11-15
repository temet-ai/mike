pub mod commits;
pub mod compose;
pub mod facts;
pub mod holdback;
pub mod questions;
pub mod rules;
pub mod state;

#[cfg(test)]
mod check_tests;
#[cfg(test)]
mod facts_tests;
#[cfg(test)]
mod holdback_tests;
#[cfg(test)]
mod tests;

use serde_json::json;
use uuid::Uuid;

use crate::llm::decisions::point::{Asked, Decider};
use crate::llm::decisions::record::DecisionSubject;
pub use compose::{HOLD_BELOW, Verdict, compose};
pub use facts::{AskSource, Facts, NudgeSource, Sources};
pub use questions::{POINT, QUESTION_SET};
pub use state::FireInput;

pub struct Subject {
    pub session_db_id: Option<i64>,
    pub operator_turn_id: Option<i64>,
    pub request_id: Option<i64>,
    pub evaluation: Option<Uuid>,
}

impl Subject {
    fn decision(&self) -> DecisionSubject {
        DecisionSubject {
            session_id: self.session_db_id,
            operator_turn_id: self.operator_turn_id,
            request_id: self.request_id,
            evaluation_uuid: self.evaluation,
            ..DecisionSubject::default()
        }
    }
}

pub struct Checked {
    pub verdict: Verdict,
    pub asked: Asked,
    pub ask_source: &'static str,
}

impl Checked {
    fn composed(&self) -> serde_json::Value {
        composed_with(&self.asked, &self.verdict.json(), self.ask_source)
    }
}

fn composed_with(
    asked: &Asked,
    verdict: &serde_json::Value,
    ask_source: &'static str,
) -> serde_json::Value {
    let mut composed = asked.composed(verdict.clone());
    composed["ask_source"] = json!(ask_source);
    composed
}

async fn settled(
    decider: &Decider,
    rule: &'static str,
    state: &serde_json::Value,
    subject: &Subject,
    ask_source: &'static str,
) -> Option<Checked> {
    let asked = rules::record(decider, rule, state, subject.decision()).await;
    let verdict = Verdict::held_by(rule);
    if decider.applies() {
        return Some(Checked {
            verdict,
            asked,
            ask_source,
        });
    }
    decider
        .mark(
            &asked,
            composed_with(&asked, &verdict.json(), ask_source),
            "none",
        )
        .await;
    None
}

async fn asked_jev(
    decider: &Decider,
    facts: &Facts,
    state: serde_json::Value,
    subject: &Subject,
) -> Option<Checked> {
    let ask_source = facts.ask_source.code();
    let asked = decider
        .ask(
            QUESTION_SET,
            state,
            questions::questions_for(facts.nudge_source),
            subject.decision(),
            json!({ "hold_below": HOLD_BELOW, "holdback_share": holdback::HOLDBACK_SHARE }),
        )
        .await;
    match compose(&asked) {
        Some(verdict) if decider.applies() => Some(Checked {
            verdict,
            asked,
            ask_source,
        }),
        other => {
            let score = other.map_or(0.0, |v| v.score);
            let composed = composed_with(&asked, &json!({ "score": score }), ask_source);
            decider.mark(&asked, composed, "none").await;
            None
        }
    }
}

pub async fn check(
    decider: &Decider,
    input: &FireInput<'_>,
    facts: &Facts,
    subject: &Subject,
) -> Option<Checked> {
    let state = state::state_with(input, facts);
    let mut checked = match rules::settle(facts) {
        Some(rule) => settled(decider, rule, &state, subject, facts.ask_source.code()).await?,
        None => asked_jev(decider, facts, state, subject).await?,
    };
    let drawn = holdback::sent_anyway(subject.evaluation);
    checked.verdict.holdback = !checked.verdict.send && drawn;
    Some(checked)
}

pub async fn mark(decider: &Decider, checked: &Checked, delivered: bool) {
    let applied = if checked.verdict.holds() {
        "held"
    } else if delivered {
        "sent"
    } else {
        "none"
    };
    decider
        .mark(&checked.asked, checked.composed(), applied)
        .await;
}
