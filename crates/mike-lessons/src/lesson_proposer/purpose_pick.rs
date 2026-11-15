use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use serde_json::{Value, json};

use super::purpose::{
    plan_moves as plan_moves_by_words, same_purpose, shares_a_word, sorted_by_project,
};
use crate::llm::decisions::env::{DecisionEnv, Pace, head_chars};
use crate::llm::decisions::point::{Asked, Decider, noul};
use crate::llm::decisions::reuse::recent_answers;
use crate::llm::decisions::{Question, record::DecisionSubject};
use crate::store::persistence::lessons::placement::{SkillPurpose, TopicMove};
use crate::store::persistence::lessons_repo::LessonsPersist as _;

pub const POINT: &str = "topic_merge";
pub const QUESTION_SET: &str = "topic_merge_v2";
pub const MIN_SAME: f64 = 0.7;
const MAX_CALLS: usize = 12;
const MAX_CANDIDATES: usize = 4;
const REMEMBER_FOR: Duration = Duration::from_secs(24 * 60 * 60);
const REUSE_WITHIN: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const SAMPLE_LESSONS: usize = 3;
const SAMPLE_CHARS: usize = 120;

type Samples = HashMap<(i64, String), Vec<String>>;

static VERDICTS: LazyLock<DashMap<String, (Instant, Option<String>)>> = LazyLock::new(DashMap::new);

fn verdict_key(topic: &SkillPurpose, candidates: &[&SkillPurpose]) -> String {
    let names: Vec<&str> = candidates.iter().map(|c| c.topic.as_str()).collect();
    format!(
        "{}\u{1f}{}\u{1f}{}",
        topic.project_id,
        topic.topic,
        names.join("\u{1f}")
    )
}

fn remembered(key: &str, now: Instant) -> Option<Option<String>> {
    VERDICTS
        .get(key)
        .filter(|entry| now.duration_since(entry.0) < REMEMBER_FOR)
        .map(|entry| entry.1.clone())
}

fn remember(key: String, survivor: Option<&SkillPurpose>, now: Instant) {
    VERDICTS.insert(key, (now, survivor.map(|s| s.topic.clone())));
}

pub fn applied_for(verdict: Option<Option<(usize, f64)>>) -> &'static str {
    match verdict {
        Some(Some(_)) => "chosen",
        Some(None) => "skipped",
        None => "none",
    }
}

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

fn first_line(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty());
    head_chars(line.unwrap_or_default(), SAMPLE_CHARS)
}

fn card(purpose: &SkillPurpose, samples: &Samples) -> Value {
    let sample = samples.get(&(purpose.project_id, purpose.topic.clone()));
    json!({
        "topic": purpose.topic,
        "lessons": purpose.lessons,
        "has_shipped_skill": purpose.shipped,
        "sample_lessons": sample.map(Vec::as_slice).unwrap_or_default(),
    })
}

pub fn state(topic: &SkillPurpose, candidates: &[&SkillPurpose], samples: &Samples) -> Value {
    json!({
        "topic": card(topic, samples),
        "candidates": candidates.iter().map(|c| card(c, samples)).collect::<Vec<_>>(),
    })
}

async fn load_samples(
    decider: &Decider,
    samples: &mut Samples,
    topic: &SkillPurpose,
    candidates: &[&SkillPurpose],
) {
    for purpose in std::iter::once(topic).chain(candidates.iter().copied()) {
        let key = (purpose.project_id, purpose.topic.clone());
        if samples.contains_key(&key) {
            continue;
        }
        let lines = match decider
            .persist()
            .lessons()
            .list_lessons_by_topic(purpose.project_id, &purpose.topic)
            .await
        {
            Ok(rows) => rows
                .iter()
                .take(SAMPLE_LESSONS)
                .map(|row| first_line(&row.text))
                .collect(),
            Err(error) => {
                tracing::warn!(%error, topic = %purpose.topic, "topic_merge: lesson samples unavailable");
                Vec::new()
            }
        };
        samples.insert(key, lines);
    }
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

fn settled<'a>(
    key: String,
    found: Option<(usize, f64)>,
    candidates: &[&'a SkillPurpose],
) -> Option<&'a SkillPurpose> {
    let survivor = found.map(|(index, _)| candidates[index]);
    remember(key, survivor, Instant::now());
    survivor
}

async fn reused(decider: &Decider, state: &Value, count: usize) -> Option<Option<(usize, f64)>> {
    let (_, asked) =
        recent_answers(decider.persist(), POINT, QUESTION_SET, state, REUSE_WITHIN).await?;
    compose(&asked, count)
}

async fn survivor_for<'a>(
    decider: &Decider,
    pace: &mut Pace,
    survivors: &[&'a SkillPurpose],
    topic: &SkillPurpose,
    samples: &mut Samples,
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
    let key = verdict_key(topic, &candidates);
    if decider.applies()
        && let Some(found) = remembered(&key, Instant::now())
    {
        return found.and_then(|name| candidates.iter().copied().find(|c| c.topic == name));
    }
    load_samples(decider, samples, topic, &candidates).await;
    let facts = state(topic, &candidates, samples);
    if decider.applies()
        && let Some(found) = reused(decider, &facts, candidates.len()).await
    {
        return settled(key, found, &candidates);
    }
    if !pace.may_ask() {
        return by_words();
    }
    let verdict = ask_verdict(decider, pace, facts, topic, &candidates).await;
    match verdict {
        Some(found) if decider.applies() => settled(key, found, &candidates),
        _ => by_words(),
    }
}

async fn ask_verdict(
    decider: &Decider,
    pace: &mut Pace,
    facts: Value,
    topic: &SkillPurpose,
    candidates: &[&SkillPurpose],
) -> Option<Option<(usize, f64)>> {
    let asked = decider
        .ask(
            QUESTION_SET,
            facts,
            questions(&topic.topic, candidates),
            DecisionSubject {
                project_id: Some(topic.project_id),
                ..DecisionSubject::default()
            },
            json!({ "min_same": MIN_SAME }),
        )
        .await;
    pace.spent(&asked);
    let verdict = compose(&asked, candidates.len());
    let score = match verdict {
        Some(Some((_, p))) => p,
        Some(None) => 1.0 - max_noul(&asked, candidates.len()),
        None => 0.0,
    };
    decider
        .mark(
            &asked,
            asked.composed(json!({ "score": score })),
            applied_for(verdict),
        )
        .await;
    verdict
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
    let mut samples = Samples::new();
    let mut plan = BTreeMap::new();
    for (project_id, topics) in sorted_by_project(purposes) {
        let mut survivors: Vec<&SkillPurpose> = Vec::new();
        let mut moves = Vec::new();
        for topic in topics {
            match survivor_for(&decider, &mut pace, &survivors, topic, &mut samples).await {
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
