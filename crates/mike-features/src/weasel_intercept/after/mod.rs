pub mod compose;
pub mod questions;

#[cfg(test)]
mod tests;

use serde_json::json;

pub use compose::{Verdict, WEASEL_AT, compose, state};
pub use questions::{POINT, QUESTION_SET};

use crate::llm::decisions::env::DecisionEnv;
use crate::llm::decisions::record::DecisionSubject;
use crate::store::models::RequestId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cut {
    pub request_id: i64,
    pub word: String,
    pub text: String,
}

pub async fn ask(env: &DecisionEnv, cut: &Cut) -> Option<Verdict> {
    let decider = env.decider(POINT)?;
    let asked = decider
        .ask(
            QUESTION_SET,
            state(&cut.word, &cut.text),
            questions::questions(),
            DecisionSubject {
                request_id: Some(cut.request_id),
                ..DecisionSubject::default()
            },
            json!({ "weasel_at": WEASEL_AT }),
        )
        .await;
    let verdict = compose(&asked);
    let recorded = verdict.filter(|_| decider.applies());
    let score = verdict.map_or(0.0, |v| v.score());
    let applied = if recorded.is_some() {
        "labelled"
    } else {
        "none"
    };
    decider
        .mark(
            &asked,
            asked.composed(json!({ "score": score, "word": cut.word })),
            applied,
        )
        .await;
    if let Some(v) = recorded {
        decider
            .outcome(
                &asked,
                v.label(),
                "jev_after_stream",
                json!({ "word": cut.word, "request_id": cut.request_id }),
            )
            .await;
    }
    verdict
}

pub fn spawn_ask(
    env: Option<DecisionEnv>,
    tracker: &tokio_util::task::TaskTracker,
    request_id: RequestId,
    word: &str,
    text: &str,
) {
    let Some(env) = env else {
        return;
    };
    let Ok(raw_id) = i64::try_from(request_id.as_u64()) else {
        return;
    };
    let cut = Cut {
        request_id: raw_id,
        word: word.to_owned(),
        text: text.to_owned(),
    };
    crate::observability::spans::spawn_tracked(tracker, async move {
        ask(&env, &cut).await;
    });
}
