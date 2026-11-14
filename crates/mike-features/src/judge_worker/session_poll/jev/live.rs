use serde_json::{Value, json};

use super::{SessionDecision, SessionJev, decide};
use crate::judge_worker::session_end::run::SessionJudgementV3;
use crate::llm::decisions::point::Decider;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::eval_repo::EvalPersist as _;

#[derive(Debug, Clone, PartialEq)]
pub struct ModelView {
    pub outcome: String,
    pub quality: f64,
}

pub async fn decide_live(
    decider: &Decider,
    persist: &PersistHandle,
    session_db_id: i64,
    judged: &SessionJudgementV3,
) -> Option<SessionJev> {
    let turns = persist
        .eval()
        .fetch_judge_digest(session_db_id)
        .await
        .ok()?;
    let first_ask = turns.first().map_or("", |t| t.user_prompt.as_str());
    decide(
        decider,
        session_db_id,
        first_ask,
        &judged.input_text,
        turns.len(),
    )
    .await
}

pub fn apply_live(judged: &mut SessionJudgementV3, decision: &SessionDecision) -> ModelView {
    let before = ModelView {
        outcome: judged.verdict.outcome.clone(),
        quality: judged.verdict.quality_score,
    };
    judged.verdict.outcome = decision.outcome.to_owned();
    judged.verdict.quality_score = decision.quality.unwrap_or(0.0);
    judged.quality_unknown = decision.quality.is_none();
    before
}

pub fn composed_live(decision: &SessionDecision, before: &ModelView) -> Value {
    let mut verdict = decision.json();
    if let Some(map) = verdict.as_object_mut() {
        map.insert(
            "model_view".to_owned(),
            json!({ "outcome": before.outcome, "quality": before.quality }),
        );
    }
    verdict
}

pub async fn mark_live(decider: &Decider, jev: &SessionJev, before: &ModelView) {
    decider
        .mark(
            &jev.asked,
            jev.asked.composed(composed_live(&jev.decision, before)),
            "sent",
        )
        .await;
}
