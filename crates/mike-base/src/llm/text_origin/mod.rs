pub mod compose;
pub mod questions;
pub mod state;

#[cfg(test)]
mod tests;

use serde_json::json;

pub use compose::{APPLY_AT, Origin, Verdict, compose};
pub use questions::{POINT, QUESTION_SET};
pub use state::Unmatched;

use crate::llm::decisions::env::DecisionEnv;
use crate::llm::decisions::record::DecisionSubject;
use crate::store::persistence::PersistHandle;

async fn write(persist: &PersistHandle, request_id: i64, verdict: &Verdict) -> bool {
    match persist
        .apply_prompt_origin(
            request_id,
            verdict.origin.kind_code(),
            verdict.clears_repeat_key(),
        )
        .await
    {
        Ok(rows) => rows > 0,
        Err(error) => {
            tracing::warn!(%error, request_id, "text origin: stored classification update failed");
            false
        }
    }
}

pub async fn classify(env: &DecisionEnv, input: &Unmatched) -> Option<Verdict> {
    let decider = env.decider(POINT)?;
    let asked = decider
        .ask(
            QUESTION_SET,
            state::state(input),
            questions::questions(),
            DecisionSubject {
                session_id: Some(input.session_db_id),
                request_id: Some(input.request_id),
                ..DecisionSubject::default()
            },
            json!({ "apply_at": APPLY_AT }),
        )
        .await;
    let verdict = compose(&asked);
    let changed = match verdict
        .as_ref()
        .filter(|v| v.applies() && decider.applies())
    {
        Some(v) => write(env.persist(), input.request_id, v).await,
        None => false,
    };
    let composed = json!({
        "score": verdict.map_or(0.0, |v| v.probability),
        "origin": verdict.map(|v| v.origin.key()),
    });
    let applied = if changed { "chosen" } else { "none" };
    decider
        .mark(&asked, asked.composed(composed), applied)
        .await;
    verdict
}
