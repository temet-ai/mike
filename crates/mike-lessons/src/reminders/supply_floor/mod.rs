pub mod compose;
pub mod questions;
pub mod state;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde_json::json;

use crate::llm::decisions::point::Decider;
use crate::llm::decisions::record::DecisionSubject;
pub use compose::{FLOOR, Supply, compose};
pub use questions::{POINT, QUESTION_SET};

const HOLD_FOR: Duration = Duration::from_secs(600);
const HELD_CAPACITY: usize = 4096;

static HELD: LazyLock<Mutex<HashMap<(String, i64), Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn is_held(session: &str, lesson_id: i64) -> bool {
    let Ok(held) = HELD.lock() else {
        return false;
    };
    held.get(&(session.to_owned(), lesson_id))
        .is_some_and(|at| at.elapsed() < HOLD_FOR)
}

fn remember_holds(session: &str, ids: &[i64]) {
    let Ok(mut held) = HELD.lock() else {
        return;
    };
    if held.len() >= HELD_CAPACITY {
        held.retain(|_, at| at.elapsed() < HOLD_FOR);
    }
    if held.len() >= HELD_CAPACITY {
        held.clear();
    }
    for id in ids {
        held.insert((session.to_owned(), *id), Instant::now());
    }
}

pub struct Context<'a> {
    pub session: &'a str,
    pub request_id: i64,
    pub text: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct Candidate<'a> {
    pub id: i64,
    pub kind: &'a str,
    pub text: &'a str,
}

pub async fn apply(
    decider: &Decider,
    context: &Context<'_>,
    lessons: &[Candidate<'_>],
) -> Vec<i64> {
    let ids: Vec<i64> = lessons.iter().map(|l| l.id).collect();
    if ids.is_empty() {
        return ids;
    }
    let asked = decider
        .ask(
            QUESTION_SET,
            state::state(context.text, lessons),
            questions::questions(lessons),
            DecisionSubject {
                request_id: Some(context.request_id),
                ..DecisionSubject::default()
            },
            json!({ "floor": FLOOR }),
        )
        .await;
    match compose(&asked, &ids) {
        Some(supply) if decider.applies() => {
            remember_holds(context.session, &supply.hold);
            let applied = if supply.hold.is_empty() {
                "sent"
            } else {
                "held"
            };
            decider
                .mark(&asked, asked.composed(supply.json()), applied)
                .await;
            supply.keep
        }
        other => {
            let score = other.map_or(0.0, |s| s.score);
            decider
                .mark(&asked, asked.composed(json!({ "score": score })), "none")
                .await;
            ids
        }
    }
}
