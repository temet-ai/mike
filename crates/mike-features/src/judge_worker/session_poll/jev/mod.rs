pub mod compose;
pub mod live;
pub mod questions;
pub mod state;

#[cfg(test)]
mod tests;

use serde_json::json;

use super::digest::JudgeVerdict;
use super::verdict_write::JudgeCall;
use crate::llm::decisions::point::{Asked, Decider};
use crate::llm::decisions::record::DecisionSubject;
pub use compose::{SessionDecision, compose};
pub use questions::{POINT, QUESTION_SET};

#[derive(Debug, Clone)]
pub struct SessionJev {
    pub decision: SessionDecision,
    pub asked: Asked,
}

pub async fn decide(
    decider: &Decider,
    session_db_id: i64,
    first_ask: &str,
    digest: &str,
    turns: usize,
) -> Option<SessionJev> {
    let asked = decider
        .ask(
            QUESTION_SET,
            state::state(first_ask, digest, turns),
            questions::questions(),
            DecisionSubject {
                session_id: Some(session_db_id),
                ..DecisionSubject::default()
            },
            json!({ "outcome_at": compose::OUTCOME_AT }),
        )
        .await;
    match compose(&asked) {
        Some(decision) if decider.applies() => Some(SessionJev { decision, asked }),
        other => {
            let score = other.as_ref().map_or(0.0, |d| d.confidence);
            decider
                .mark(&asked, asked.composed(json!({ "score": score })), "none")
                .await;
            None
        }
    }
}

pub fn apply(verdict: &mut JudgeVerdict, decision: &SessionDecision) {
    verdict.outcome = decision.outcome.to_owned();
    verdict.quality_score = decision.quality.unwrap_or(0.0);
    verdict.quality_unknown = decision.quality.is_none();
}

pub fn synthesized(decision: &SessionDecision) -> JudgeVerdict {
    let mut verdict = JudgeVerdict {
        outcome: String::new(),
        quality_score: 0.0,
        quality_unknown: false,
        rationale: decision.summary(),
        overview: String::new(),
        topics: Vec::new(),
        learnings: Vec::new(),
    };
    apply(&mut verdict, decision);
    verdict
}

pub fn reconcile(call: JudgeCall, jev: Option<&SessionJev>) -> JudgeCall {
    match (call, jev) {
        (JudgeCall::Verdict(mut verdict), Some(j)) => {
            apply(&mut verdict, &j.decision);
            JudgeCall::Verdict(verdict)
        }
        (JudgeCall::Failed, Some(j)) => JudgeCall::Verdict(synthesized(&j.decision)),
        (call, _) => call,
    }
}

pub async fn mark_stored(decider: &Decider, jev: &SessionJev) {
    decider
        .mark(&jev.asked, jev.asked.composed(jev.decision.json()), "sent")
        .await;
}
