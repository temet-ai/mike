pub mod compose;
pub mod questions;
pub mod state;

#[cfg(test)]
mod tests;

use serde_json::json;

use crate::llm::decisions::point::Decider;
use crate::llm::decisions::record::DecisionSubject;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::dossier_repo::DossierPersist as _;
use crate::store::persistence::operator_turns::ListTurnsFilter;
pub use compose::{CORRECTION_AT, Classified, compose};
pub use questions::{POINT, QUESTION_SET};

pub struct Message<'a> {
    pub session_db_id: i64,
    pub request_id: i64,
    pub operator_text: &'a str,
    pub prior_turn: &'a str,
}

pub async fn classify(decider: &Decider, message: &Message<'_>) -> Option<Classified> {
    let asked = decider
        .ask(
            QUESTION_SET,
            state::state(message.operator_text, message.prior_turn),
            questions::questions(),
            DecisionSubject {
                session_id: Some(message.session_db_id),
                request_id: Some(message.request_id),
                ..DecisionSubject::default()
            },
            json!({ "correction_at": CORRECTION_AT }),
        )
        .await;
    match compose(&asked) {
        Some(classified) if decider.applies() => {
            let applied = if classified.kind == "correction" {
                "warned"
            } else {
                "passed"
            };
            decider
                .mark(&asked, asked.composed(classified.json()), applied)
                .await;
            Some(classified)
        }
        other => {
            let score = other.as_ref().map_or(0.0, |c| c.probability);
            decider
                .mark(&asked, asked.composed(json!({ "score": score })), "none")
                .await;
            None
        }
    }
}

pub async fn prior_turn_end(
    persist: &PersistHandle,
    session_db_id: i64,
    before_request_id: i64,
) -> Option<String> {
    let filter = ListTurnsFilter {
        session_db_id: Some(session_db_id),
        limit: 3,
        ..ListTurnsFilter::default()
    };
    let turns = persist.list_turns(&filter).await.ok()?;
    let turn = turns
        .into_iter()
        .find(|t| t.start_request_id < before_request_id)?;
    let closing = turn.stop_response.filter(|s| !s.trim().is_empty());
    Some(closing.unwrap_or(turn.final_assistant_text))
}

pub async fn store(persist: &PersistHandle, request_id: i64, classified: &Classified) {
    if let Err(error) = persist
        .dossier()
        .set_ask_kind_by_judge(request_id, classified.kind)
        .await
    {
        tracing::warn!(request_id, %error, "correction classifier: ask kind not stored");
    }
}
