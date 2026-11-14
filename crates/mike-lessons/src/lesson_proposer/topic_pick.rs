use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::llm::decisions::env::{DecisionEnv, Pace, head_chars};
use crate::llm::decisions::point::{Asked, choice};
use crate::llm::decisions::{Question, record::DecisionSubject};
use crate::store::persistence::lessons::topics::UntopicedLesson;

pub const POINT: &str = "topic_assign";
pub const QUESTION_SET: &str = "topic_assign_v1";
pub const NEW_KEY: &str = "new";
pub const MIN_TOPIC: f64 = 0.5;
const GROUP: usize = 5;
const MAX_CALLS: usize = 6;
const MAX_TOPICS: usize = 60;
const TEXT_CHARS: usize = 500;

fn question_key(id: i64) -> String {
    format!("lesson_{id}")
}

pub fn questions(existing: &[String], group: &[&UntopicedLesson]) -> BTreeMap<String, Question> {
    let mut options: Vec<(&str, String)> = existing
        .iter()
        .map(|topic| (topic.as_str(), format!("lessons about {topic}")))
        .collect();
    options.push((
        NEW_KEY,
        "none of the existing topics covers the area of work this lesson concerns".to_owned(),
    ));
    let refs: Vec<(&str, &str)> = options.iter().map(|(k, v)| (*k, v.as_str())).collect();
    group
        .iter()
        .map(|lesson| {
            (
                question_key(lesson.id),
                choice(
                    &format!(
                        "Which existing topic does lesson {} belong to? A topic names an area of work, so that lessons about the same work share one topic.",
                        lesson.id
                    ),
                    &refs,
                ),
            )
        })
        .collect()
}

pub fn state(existing: &[String], group: &[&UntopicedLesson]) -> Value {
    json!({
        "existing_topics": existing,
        "lessons": group
            .iter()
            .map(|l| json!({ "id": l.id, "text": head_chars(&l.text, TEXT_CHARS) }))
            .collect::<Vec<_>>(),
    })
}

pub fn compose(
    asked: &Asked,
    group: &[&UntopicedLesson],
    existing: &[String],
) -> Vec<(i64, String, f64)> {
    if asked.failed() {
        return Vec::new();
    }
    group
        .iter()
        .filter_map(|lesson| {
            let (topic, p) = asked.choice(&question_key(lesson.id))?;
            let known = existing.iter().any(|t| t == topic);
            (known && p >= MIN_TOPIC).then(|| (lesson.id, topic.to_owned(), p))
        })
        .collect()
}

pub struct Assigned {
    pub ids: Vec<i64>,
    pub topics: Vec<String>,
}

pub async fn assign_to_existing(
    env: Option<&DecisionEnv>,
    existing: &[String],
    batch: &[UntopicedLesson],
) -> Assigned {
    let mut out = Assigned {
        ids: Vec::new(),
        topics: Vec::new(),
    };
    let Some(decider) = env.and_then(|e| e.decider(POINT)) else {
        return out;
    };
    if existing.is_empty() {
        return out;
    }
    let shown: Vec<String> = existing.iter().take(MAX_TOPICS).cloned().collect();
    let lessons: Vec<&UntopicedLesson> = batch.iter().collect();
    let mut pace = Pace::new(MAX_CALLS);
    for group in lessons.chunks(GROUP) {
        if !pace.may_ask() {
            break;
        }
        let asked = decider
            .ask(
                QUESTION_SET,
                state(&shown, group),
                questions(&shown, group),
                DecisionSubject::default(),
                json!({ "min_topic": MIN_TOPIC }),
            )
            .await;
        pace.spent(&asked);
        let picked = compose(&asked, group, &shown);
        let score = if picked.is_empty() {
            0.0
        } else {
            picked.iter().map(|(_, _, p)| p).sum::<f64>() / picked.len() as f64
        };
        let applied = if picked.is_empty() { "new" } else { "assigned" };
        decider
            .mark(&asked, asked.composed(json!({ "score": score })), applied)
            .await;
        if decider.applies() {
            for (id, topic, _) in picked {
                out.ids.push(id);
                out.topics.push(topic);
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "topic_pick_tests.rs"]
mod tests;
