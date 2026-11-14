use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use serde_json::{Value, json};

use super::first_ten::ResultGrade;
use crate::llm::decisions::env::{DecisionEnv, head_chars};
use crate::llm::decisions::point::{Asked, Decider, noul};
use crate::llm::decisions::{Question, record::DecisionSubject};

pub const POINT: &str = "search_grade";
pub const QUESTION_SET: &str = "search_grade_v1";
pub const MIN_RELEVANT: f64 = 0.5;
const TEXT_CHARS: usize = 500;
const QUERY_CHARS: usize = 400;

pub struct SearchJev {
    decider: Decider,
    left: AtomicUsize,
    tripped: AtomicBool,
}

impl SearchJev {
    pub fn new(env: Option<&DecisionEnv>, max_calls: usize) -> Option<Self> {
        Some(Self {
            decider: env?.decider(POINT)?,
            left: AtomicUsize::new(max_calls),
            tripped: AtomicBool::new(false),
        })
    }

    fn take(&self) -> bool {
        if self.tripped.load(Ordering::Relaxed) {
            return false;
        }
        self.left
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_sub(1))
            .is_ok()
    }
}

fn question_key(position: usize) -> String {
    format!("result_{position}")
}

pub struct Shown<'a> {
    pub position: usize,
    pub reference: &'a str,
    pub text: &'a str,
    pub query_score: Option<f64>,
    pub turn_score: Option<f64>,
}

pub fn questions(shown: &[Shown<'_>], has_turn: bool) -> BTreeMap<String, Question> {
    let ask = if has_turn {
        "Does the result answer the search query, or the question the operator asked in the turn?"
    } else {
        "Does the result answer the search query?"
    };
    shown
        .iter()
        .map(|s| {
            (
                question_key(s.position),
                noul(
                    &format!("{ask} (result {})", s.position),
                    "the result contains what the query or the turn needs",
                    "the result is about something else or only shares words with the query",
                ),
            )
        })
        .collect()
}

pub fn state(query: &str, turn_prompt: Option<&str>, shown: &[Shown<'_>]) -> Value {
    json!({
        "query": head_chars(query, QUERY_CHARS),
        "turn_prompt": turn_prompt.map(|t| head_chars(t, QUERY_CHARS)),
        "results": shown
            .iter()
            .map(|s| json!({
                "position": s.position,
                "text": head_chars(s.text, TEXT_CHARS),
                "reranker_score_for_query": s.query_score,
                "reranker_score_for_turn": s.turn_score,
            }))
            .collect::<Vec<_>>(),
    })
}

pub fn compose(asked: &Asked, shown: &[Shown<'_>]) -> Option<Vec<(usize, f64)>> {
    if asked.failed() {
        return None;
    }
    shown
        .iter()
        .map(|s| Some((s.position, asked.noul(&question_key(s.position))?)))
        .collect()
}

fn mean_applied(answers: &[(usize, f64)]) -> f64 {
    if answers.is_empty() {
        return 0.0;
    }
    answers
        .iter()
        .map(|(_, p)| if *p >= MIN_RELEVANT { *p } else { 1.0 - *p })
        .sum::<f64>()
        / answers.len() as f64
}

pub async fn grade(
    jev: &SearchJev,
    query: &str,
    turn_prompt: Option<&str>,
    results: &mut [ResultGrade],
    texts: &[Option<&str>],
    used: &[String],
) {
    if !jev.take() {
        return;
    }
    let shown: Vec<Shown<'_>> = results
        .iter()
        .zip(texts)
        .filter_map(|(r, text)| {
            Some(Shown {
                position: r.position,
                reference: &r.reference,
                text: (*text)?,
                query_score: r.query_score,
                turn_score: r.turn_score,
            })
        })
        .collect();
    if shown.is_empty() {
        return;
    }
    let asked = jev
        .decider
        .ask(
            QUESTION_SET,
            state(query, turn_prompt, &shown),
            questions(&shown, turn_prompt.is_some()),
            DecisionSubject::default(),
            json!({ "min_relevant": MIN_RELEVANT }),
        )
        .await;
    if asked.failed() {
        jev.tripped.store(true, Ordering::Relaxed);
    }
    let answers = compose(&asked, &shown);
    let score = answers.as_deref().map_or(0.0, mean_applied);
    let applied = if answers.is_some() { "graded" } else { "none" };
    jev.decider
        .mark(&asked, asked.composed(json!({ "score": score })), applied)
        .await;
    let Some(answers) = answers.filter(|_| jev.decider.applies()) else {
        return;
    };
    let used_positions: Vec<(usize, f64)> = shown
        .iter()
        .filter(|s| used.iter().any(|u| u == s.reference))
        .filter_map(|s| answers.iter().find(|(p, _)| *p == s.position).copied())
        .collect();
    for (position, p) in &answers {
        if let Some(result) = results.iter_mut().find(|r| r.position == *position) {
            result.relevant = Some(*p >= MIN_RELEVANT);
        }
    }
    for (position, p) in used_positions {
        let label = if p >= MIN_RELEVANT { "right" } else { "wrong" };
        jev.decider
            .outcome(
                &asked,
                label,
                "search_used_reference",
                json!({ "position": position, "probability": p }),
            )
            .await;
    }
}

#[cfg(test)]
#[path = "search_decide_tests.rs"]
mod tests;
