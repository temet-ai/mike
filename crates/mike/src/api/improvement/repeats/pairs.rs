use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::llm::decisions::env::{DecisionEnv, Pace, head_chars};
use crate::llm::decisions::point::{Asked, noul};
use crate::llm::decisions::{Question, record::DecisionSubject};
use crate::store::persistence::improvement::repeats::PushbackPrompt;

pub(super) const POINT: &str = "repeats_group";
pub(super) const QUESTION_SET: &str = "repeats_group_v1";
pub(super) const MIN_SAME: f64 = 0.5;
const BAND_BELOW: f32 = 0.10;
const BAND_ABOVE: f32 = 0.05;
const GROUP: usize = 5;
const MAX_CALLS: usize = 8;
const TEXT_CHARS: usize = 400;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Pair {
    pub i: usize,
    pub j: usize,
    pub cosine: f32,
}

fn question_key(n: usize) -> String {
    format!("pair_{n}")
}

pub(super) fn borderline(sim: &[Vec<f32>], threshold: f32) -> Vec<Pair> {
    let mut pairs = Vec::new();
    for (i, row) in sim.iter().enumerate() {
        for (j, cosine) in row.iter().enumerate().skip(i + 1) {
            if *cosine >= threshold - BAND_BELOW && *cosine <= threshold + BAND_ABOVE {
                pairs.push(Pair {
                    i,
                    j,
                    cosine: *cosine,
                });
            }
        }
    }
    pairs.sort_by(|a, b| {
        (a.cosine - threshold)
            .abs()
            .total_cmp(&(b.cosine - threshold).abs())
            .then(a.i.cmp(&b.i))
            .then(a.j.cmp(&b.j))
    });
    pairs.truncate(GROUP * MAX_CALLS);
    pairs
}

pub(super) fn questions(count: usize) -> BTreeMap<String, Question> {
    (0..count)
        .map(|n| {
            (
                question_key(n),
                noul(
                    &format!("Is message B of pair {n} the same request as message A, restated?"),
                    "both messages ask for the same thing, in different words or after a first attempt failed",
                    "the messages ask for different things, even if they share a topic",
                ),
            )
        })
        .collect()
}

pub(super) fn state(group: &[(&Pair, &PushbackPrompt, &PushbackPrompt)]) -> Value {
    json!({
        "pairs": group
            .iter()
            .enumerate()
            .map(|(n, (pair, a, b))| json!({
                "pair": n,
                "message_a": head_chars(&a.text, TEXT_CHARS),
                "message_b": head_chars(&b.text, TEXT_CHARS),
                "cosine": (f64::from(pair.cosine) * 1000.0).round() / 1000.0,
                "same_session": a.session_id == b.session_id,
                "days_apart": (a.at - b.at).num_days().abs(),
            }))
            .collect::<Vec<_>>(),
    })
}

pub(super) fn compose(asked: &Asked, count: usize) -> Option<Vec<f64>> {
    if asked.failed() {
        return None;
    }
    (0..count).map(|n| asked.noul(&question_key(n))).collect()
}

fn applied_score(answers: &[f64]) -> f64 {
    if answers.is_empty() {
        return 0.0;
    }
    answers
        .iter()
        .map(|p| if *p >= MIN_SAME { *p } else { 1.0 - *p })
        .sum::<f64>()
        / answers.len() as f64
}

pub(super) async fn refine(
    env: Option<&DecisionEnv>,
    kept: &[&PushbackPrompt],
    sim: &mut [Vec<f32>],
    threshold: f32,
) {
    let Some(decider) = env.and_then(|e| e.decider(POINT)) else {
        return;
    };
    let pairs = borderline(sim, threshold);
    let mut pace = Pace::new(MAX_CALLS);
    for chunk in pairs.chunks(GROUP) {
        if !pace.may_ask() {
            break;
        }
        let group: Vec<(&Pair, &PushbackPrompt, &PushbackPrompt)> =
            chunk.iter().map(|p| (p, kept[p.i], kept[p.j])).collect();
        let asked = decider
            .ask(
                QUESTION_SET,
                state(&group),
                questions(group.len()),
                DecisionSubject::default(),
                json!({ "min_same": MIN_SAME, "cosine_threshold": threshold }),
            )
            .await;
        pace.spent(&asked);
        let answers = compose(&asked, group.len());
        let applied = if answers.is_some() { "grouped" } else { "none" };
        let score = answers.as_deref().map_or(0.0, applied_score);
        decider
            .mark(&asked, asked.composed(json!({ "score": score })), applied)
            .await;
        let Some(answers) = answers.filter(|_| decider.applies()) else {
            continue;
        };
        for (pair, p) in chunk.iter().zip(answers) {
            let value = if p >= MIN_SAME { 1.0 } else { 0.0 };
            sim[pair.i][pair.j] = value;
            sim[pair.j][pair.i] = value;
        }
    }
}

#[cfg(test)]
#[path = "pairs_tests.rs"]
mod tests;
