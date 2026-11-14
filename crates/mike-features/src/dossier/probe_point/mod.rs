pub mod compose;
pub mod questions;
pub mod state;

#[cfg(test)]
mod tests;

use std::sync::LazyLock;

use dashmap::DashMap;
use serde_json::json;

pub use compose::{PROBE_AT, Verdict, compose};
pub use questions::{POINT, QUESTION_SET};
pub use state::{Candidate, Reason};

use crate::llm::decisions::env::{DecisionEnv, Pace};
use crate::llm::decisions::record::DecisionSubject;

const GROUP: usize = 5;
const MAX_CALLS: usize = 4;
const MAX_REMEMBERED: usize = 20_000;

static VERDICTS: LazyLock<DashMap<i64, bool>> = LazyLock::new(DashMap::new);

pub fn verdict(session_id: i64) -> Option<bool> {
    VERDICTS.get(&session_id).map(|v| *v)
}

fn remember(found: &[Verdict]) {
    if VERDICTS.len() > MAX_REMEMBERED {
        VERDICTS.clear();
    }
    for v in found {
        VERDICTS.insert(v.session_id, v.is_probe);
    }
}

fn pending(candidates: Vec<Candidate>) -> Vec<Candidate> {
    candidates
        .into_iter()
        .filter(|c| !VERDICTS.contains_key(&c.session_id))
        .take(GROUP * MAX_CALLS)
        .collect()
}

pub async fn fill(env: &DecisionEnv, candidates: Vec<Candidate>) {
    let wanted = pending(candidates);
    if wanted.is_empty() {
        return;
    }
    let Some(decider) = env.decider(POINT) else {
        return;
    };
    let refs: Vec<&Candidate> = wanted.iter().collect();
    let mut pace = Pace::new(MAX_CALLS);
    for group in refs.chunks(GROUP) {
        if !pace.may_ask() {
            break;
        }
        let asked = decider
            .ask(
                QUESTION_SET,
                state::state(group),
                questions::questions(group),
                DecisionSubject {
                    session_id: group.first().map(|c| c.session_id),
                    ..DecisionSubject::default()
                },
                json!({ "probe_at": PROBE_AT }),
            )
            .await;
        pace.spent(&asked);
        let found = compose(&asked, group);
        let score = if found.is_empty() {
            0.0
        } else {
            found.iter().map(Verdict::score).sum::<f64>() / found.len() as f64
        };
        let applied = if found.is_empty() || !decider.applies() {
            "none"
        } else {
            remember(&found);
            "chosen"
        };
        decider
            .mark(&asked, asked.composed(json!({ "score": score })), applied)
            .await;
    }
}
