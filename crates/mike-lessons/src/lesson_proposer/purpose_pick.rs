use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::purpose::{
    plan_moves as plan_moves_by_words, same_purpose, shares_a_word, sorted_by_project,
};
use crate::llm::decisions::env::{DecisionEnv, Pace};
use crate::llm::decisions::point::{Asked, Decider, noul};
use crate::llm::decisions::{Question, record::DecisionSubject};
use crate::store::persistence::lessons::placement::{SkillPurpose, TopicMove};

pub const POINT: &str = "topic_merge";
pub const QUESTION_SET: &str = "topic_merge_v1";
pub const MIN_SAME: f64 = 0.7;
const MAX_CALLS: usize = 12;
const MAX_CANDIDATES: usize = 4;

fn question_key(index: usize) -> String {
    format!("candidate_{index}")
}

pub fn questions(topic: &str, candidates: &[&SkillPurpose]) -> BTreeMap<String, Question> {
    candidates
        .iter()
        .enumerate()
        .map(|(index, other)| {
            (
                question_key(index),
                noul(
                    &format!(
                        "Do the topics \"{topic}\" and \"{}\" cover the same area of work, so that lessons filed under one belong under the other?",
                        other.topic
                    ),
                    "both topics name the same area of work",
                    "the topics name different areas of work",
                ),
            )
        })
        .collect()
}

fn card(purpose: &SkillPurpose) -> Value {
    json!({ "topic": purpose.topic, "lessons": purpose.lessons, "has_shipped_skill": purpose.shipped })
}

pub fn state(topic: &SkillPurpose, candidates: &[&SkillPurpose]) -> Value {
    json!({
        "topic": card(topic),
        "candidates": candidates.iter().map(|c| card(c)).collect::<Vec<_>>(),
    })
}

pub fn compose(asked: &Asked, count: usize) -> Option<Option<(usize, f64)>> {
    if asked.failed() {
        return None;
    }
    let best = (0..count)
        .filter_map(|index| Some((index, asked.noul(&question_key(index))?)))
        .filter(|(_, p)| *p >= MIN_SAME)
        .max_by(|a, b| a.1.total_cmp(&b.1));
    Some(best)
}

async fn survivor_for<'a>(
    decider: &Decider,
    pace: &mut Pace,
    survivors: &[&'a SkillPurpose],
    topic: &SkillPurpose,
) -> Option<&'a SkillPurpose> {
    let candidates: Vec<&'a SkillPurpose> = survivors
        .iter()
        .copied()
        .filter(|s| shares_a_word(&s.topic, &topic.topic))
        .take(MAX_CANDIDATES)
        .collect();
    if candidates.is_empty() {
        return None;
    }
    let by_words = || {
        candidates
            .iter()
            .copied()
            .find(|s| same_purpose(&s.topic, &topic.topic))
    };
    if !pace.may_ask() {
        return by_words();
    }
    let asked = decider
        .ask(
            QUESTION_SET,
            state(topic, &candidates),
            questions(&topic.topic, &candidates),
            DecisionSubject {
                project_id: Some(topic.project_id),
                ..DecisionSubject::default()
            },
            json!({ "min_same": MIN_SAME }),
        )
        .await;
    pace.spent(&asked);
    let verdict = compose(&asked, candidates.len());
    let (applied, score) = match verdict {
        Some(Some((_, p))) => ("chosen", p),
        Some(None) => ("none", 1.0 - max_noul(&asked, candidates.len())),
        None => ("none", 0.0),
    };
    decider
        .mark(&asked, asked.composed(json!({ "score": score })), applied)
        .await;
    match verdict {
        Some(found) if decider.applies() => found.map(|(index, _)| candidates[index]),
        _ => by_words(),
    }
}

fn max_noul(asked: &Asked, count: usize) -> f64 {
    (0..count)
        .filter_map(|index| asked.noul(&question_key(index)))
        .fold(0.0, f64::max)
}

pub async fn plan_moves(
    env: Option<&DecisionEnv>,
    purposes: &[SkillPurpose],
) -> BTreeMap<i64, Vec<TopicMove>> {
    let Some(decider) = env.and_then(|e| e.decider(POINT)) else {
        return plan_moves_by_words(purposes);
    };
    let mut pace = Pace::new(MAX_CALLS);
    let mut plan = BTreeMap::new();
    for (project_id, topics) in sorted_by_project(purposes) {
        let mut survivors: Vec<&SkillPurpose> = Vec::new();
        let mut moves = Vec::new();
        for topic in topics {
            match survivor_for(&decider, &mut pace, &survivors, topic).await {
                Some(survivor) => moves.push(TopicMove {
                    from: topic.topic.clone(),
                    to: survivor.topic.clone(),
                }),
                None => survivors.push(topic),
            }
        }
        if !moves.is_empty() {
            plan.insert(project_id, moves);
        }
    }
    plan
}

#[cfg(test)]
#[path = "purpose_pick_tests.rs"]
mod tests;
