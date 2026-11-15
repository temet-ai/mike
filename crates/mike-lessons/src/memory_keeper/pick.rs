use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::resolve::{Pick, Resolution, ResolveInput, ThreadFacts, ranked_candidates};
use super::resolve_text::typed_text;
use crate::llm::decisions::env::{DecisionEnv, head_chars, tail_chars};
use crate::llm::decisions::operator_text::typed_by_operator;
use crate::llm::decisions::point::{Asked, Decider, choice, noul};
use crate::llm::decisions::{Question, record::DecisionSubject};

pub const POINT: &str = "thread_pick";
pub const QUESTION_SET: &str = "thread_pick_v3";
pub const NONE_KEY: &str = "none";
pub const MIN_PICK: f64 = 0.5;
pub const ABOUT_AT: f64 = 0.7;
const QUESTION: &str = "thread";
const ABOUT: &str = "about_a_candidate_thread";
const MAX_CANDIDATES: usize = 6;
const PROMPT_CHARS: usize = 600;
const TURN_CHARS: usize = 300;
const LABEL_CHARS: usize = 160;
const SECONDS_PER_DAY: f64 = 86_400.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Picked {
    pub resolution: Resolution,
    pub score: f64,
    pub applied: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Decided {
    pub resolution: Resolution,
    pub record_id: Option<i64>,
}

fn label(thread: Option<&ThreadFacts>, slug: &str) -> String {
    match thread {
        Some(t) => head_chars(&format!("{}: {}", t.title, t.goal), LABEL_CHARS),
        None => slug.to_owned(),
    }
}

pub fn questions(candidates: &[Pick], threads: &[ThreadFacts]) -> BTreeMap<String, Question> {
    let mut options: Vec<(String, String)> = candidates
        .iter()
        .map(|pick| {
            let thread = threads.iter().find(|t| t.slug == pick.slug);
            (pick.slug.clone(), label(thread, &pick.slug))
        })
        .collect();
    options.push((
        NONE_KEY.to_owned(),
        "the message continues none of these threads: a new topic, a general question or unrelated work"
            .to_owned(),
    ));
    let refs: Vec<(&str, &str)> = options
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    BTreeMap::from([
        (
            QUESTION.to_owned(),
            choice(
                "Which thread's work does `operator_message` continue?",
                &refs,
            ),
        ),
        (
            ABOUT.to_owned(),
            noul(
                "Is `operator_message` about the work of any thread in `candidate_threads`?",
                "The message is about the work of one of the candidate threads.",
                "The message shares at most a word with the candidate threads, asks a general question or starts unrelated work.",
            ),
        ),
    ])
}

fn days_since(thread: Option<&ThreadFacts>, now: DateTime<Utc>) -> Option<f64> {
    let seconds = now
        .signed_duration_since(thread?.last_checkpoint_at)
        .num_seconds()
        .max(0);
    Some((seconds as f64 / SECONDS_PER_DAY * 10.0).round() / 10.0)
}

pub fn state(input: &ResolveInput<'_>, message: &str, candidates: &[Pick]) -> Value {
    let turns: Vec<String> = input
        .session
        .recent_turn_text
        .iter()
        .rev()
        .filter_map(|turn| typed_by_operator(turn))
        .take(3)
        .map(|turn| tail_chars(&turn, TURN_CHARS))
        .collect();
    let cards: Vec<Value> = candidates
        .iter()
        .map(|pick| {
            let thread = input.threads.iter().find(|t| t.slug == pick.slug);
            json!({
                "slug": pick.slug,
                "title": thread.map(|t| t.title.as_str()),
                "goal": thread.map(|t| tail_chars(&t.goal, LABEL_CHARS)),
                "status": thread.map(|t| t.status.as_str()),
                "days_since_checkpoint": days_since(thread, input.now),
            })
        })
        .collect();
    json!({
        "operator_message": tail_chars(message, PROMPT_CHARS),
        "recent_turns_newest_first": turns,
        "candidate_threads": cards,
    })
}

pub fn compose(asked: &Asked, candidates: &[Pick]) -> Option<Picked> {
    if asked.failed() {
        return None;
    }
    let about = asked.noul(ABOUT)?;
    if about < ABOUT_AT {
        return Some(Picked {
            resolution: Resolution::None,
            score: 1.0 - about,
            applied: "skipped",
        });
    }
    let (key, probability) = asked.choice(QUESTION)?;
    if key == NONE_KEY {
        return Some(Picked {
            resolution: Resolution::None,
            score: probability,
            applied: "skipped",
        });
    }
    let pick = candidates.iter().find(|p| p.slug == key)?;
    if probability < MIN_PICK {
        return Some(Picked {
            resolution: Resolution::None,
            score: 1.0 - probability,
            applied: "skipped",
        });
    }
    Some(Picked {
        resolution: Resolution::One(pick.clone()),
        score: probability,
        applied: "chosen",
    })
}

async fn ask(decider: &Decider, input: &ResolveInput<'_>, message: &str) -> Option<Decided> {
    let candidates = ranked_candidates(input, MAX_CANDIDATES);
    if candidates.is_empty() {
        return None;
    }
    let asked = decider
        .ask(
            QUESTION_SET,
            state(input, message, &candidates),
            questions(&candidates, input.threads),
            DecisionSubject::default(),
            json!({ "min_pick": MIN_PICK, "max_candidates": MAX_CANDIDATES }),
        )
        .await;
    let picked = compose(&asked, &candidates);
    let applied = picked.as_ref().map_or("none", |p| p.applied);
    let score = picked.as_ref().map_or(0.0, |p| p.score);
    decider
        .mark(&asked, asked.composed(json!({ "score": score })), applied)
        .await;
    let picked = picked.filter(|_| decider.applies())?;
    Some(Decided {
        resolution: picked.resolution,
        record_id: asked.record_id,
    })
}

pub async fn decide(
    env: Option<&DecisionEnv>,
    input: &ResolveInput<'_>,
    today: Resolution,
) -> Decided {
    let off = Decided {
        resolution: today,
        record_id: None,
    };
    let Some(decider) = env.and_then(|e| e.decider(POINT)).map(Decider::inline) else {
        return off;
    };
    let Some(message) = input.prompt.and_then(typed_text) else {
        return off;
    };
    ask(&decider, input, &message).await.unwrap_or(off)
}

#[cfg(test)]
#[path = "pick_tests.rs"]
mod tests;
