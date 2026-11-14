pub mod compose;
pub mod git;
pub mod questions;
pub mod state;

#[cfg(test)]
mod tests;

use serde_json::json;

use crate::llm::decisions::point::{Asked, Decider};
use crate::llm::decisions::record::DecisionSubject;
pub use compose::{Warning, compose};
pub use questions::{POINT, QUESTION_SET};
pub use state::Commit;

pub struct Checked {
    pub warning: Warning,
    pub asked: Asked,
}

pub async fn check(
    decider: &Decider,
    commit: &Commit,
    mut subject: DecisionSubject,
) -> Option<Checked> {
    subject.commit_sha = Some(commit.sha.clone());
    let asked = decider
        .ask(
            QUESTION_SET,
            state::state(commit),
            questions::questions(),
            subject,
            json!({
                "describes_below": compose::DESCRIBES_BELOW,
                "unmentioned_at": compose::UNMENTIONED_AT,
            }),
        )
        .await;
    match compose(&asked) {
        Some(warning) if decider.applies() => Some(Checked { warning, asked }),
        other => {
            let score = other.map_or(0.0, |w| w.score());
            decider
                .mark(&asked, asked.composed(json!({ "score": score })), "none")
                .await;
            None
        }
    }
}

pub async fn mark(decider: &Decider, checked: &Checked, delivered: bool) {
    let applied = if checked.warning.warn && delivered {
        "warned"
    } else {
        "none"
    };
    decider
        .mark(
            &checked.asked,
            checked.asked.composed(checked.warning.json(delivered)),
            applied,
        )
        .await;
}
