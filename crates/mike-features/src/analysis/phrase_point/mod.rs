pub mod compose;
pub mod questions;
pub mod state;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use serde_json::json;
use tokio::sync::RwLock;

pub use compose::{EXCUSE_AT, compose, keep, mean_confidence};
pub use questions::{POINT, QUESTION_SET};

use crate::llm::decisions::env::DecisionEnv;
use crate::llm::decisions::point::Decider;
use crate::llm::decisions::record::DecisionSubject;
use crate::store::RequestStore;
use crate::store::models::{Diagnostic, RequestId};

pub async fn refine(
    decider: &Decider,
    text: &str,
    diagnostics: &[Diagnostic],
    subject: DecisionSubject,
) -> Option<Vec<bool>> {
    if diagnostics.is_empty() {
        return None;
    }
    let asked = decider
        .ask(
            QUESTION_SET,
            state::state(text, diagnostics),
            questions::questions(diagnostics.len()),
            subject,
            json!({ "excuse_at": EXCUSE_AT }),
        )
        .await;
    let excuses = compose(&asked, diagnostics.len());
    let score = mean_confidence(&asked, diagnostics.len());
    let used = if decider.applies() { excuses } else { None };
    let dropped = used
        .as_ref()
        .is_some_and(|flags| flags.iter().any(|excuse| !excuse));
    let applied = if dropped { "skipped" } else { "none" };
    decider
        .mark(&asked, asked.composed(json!({ "score": score })), applied)
        .await;
    used
}

pub async fn filter_stored(
    env: &DecisionEnv,
    store: &Arc<RwLock<RequestStore>>,
    request_id: RequestId,
    text: &str,
) {
    let Some(decider) = env.decider(POINT) else {
        return;
    };
    let found = {
        let guard = store.read().await;
        guard
            .get(&request_id)
            .map(|req| (req.session_id.clone(), req.diagnostics.clone()))
    };
    let Some((session_id, diagnostics)) = found else {
        return;
    };
    let subject = DecisionSubject {
        request_id: i64::try_from(request_id.as_u64()).ok(),
        ..DecisionSubject::default()
    };
    let Some(excuses) = refine(&decider, text, &diagnostics, subject).await else {
        return;
    };
    let kept = keep(diagnostics, &excuses);
    let mut guard = store.write().await;
    guard.update(&request_id, |req| req.diagnostics = kept.clone());
    if guard
        .latest_findings(&session_id)
        .is_some_and(|(latest, _)| latest == request_id)
    {
        guard.record_latest_findings(&session_id, request_id, &kept);
    }
}
