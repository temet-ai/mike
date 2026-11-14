use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::llm::decisions::env::{DecisionEnv, tail_chars};
use crate::llm::decisions::point::{Asked, choice};
use crate::llm::decisions::{Question, record::DecisionSubject};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::lessons_repo::LessonsPersist as _;

pub const POINT: &str = "recall_topic";
pub const QUESTION_SET: &str = "recall_topic_v1";
pub const NONE_KEY: &str = "none";
pub const MIN_MAP: f64 = 0.6;
const MAX_TYPED: usize = 3;
const MAX_CATALOG: usize = 60;
const CONTEXT_CHARS: usize = 600;

#[derive(Debug, Clone, PartialEq)]
pub struct Mapping {
    pub typed: String,
    pub mapped: String,
    pub probability: f64,
}

fn key(index: usize) -> String {
    format!("typed_{index}")
}

pub fn questions(missing: &[String], catalog: &[String]) -> BTreeMap<String, Question> {
    let mut options: Vec<(&str, String)> = catalog
        .iter()
        .map(|topic| (topic.as_str(), format!("lessons filed under {topic}")))
        .collect();
    options.push((
        NONE_KEY,
        "no catalog topic covers the same area of work".to_owned(),
    ));
    let refs: Vec<(&str, &str)> = options.iter().map(|(k, v)| (*k, v.as_str())).collect();
    missing
        .iter()
        .enumerate()
        .map(|(index, typed)| {
            (
                key(index),
                choice(
                    &format!(
                        "The session asked to recall the topic \"{typed}\", which no lesson is filed under. Which catalog topic did it mean? Pick one only when it covers the same area of work."
                    ),
                    &refs,
                ),
            )
        })
        .collect()
}

pub fn state(missing: &[String], catalog: &[String], context: &str) -> Value {
    json!({
        "typed_topics": missing,
        "catalog_topics": catalog,
        "session_text_before_the_request": tail_chars(context, CONTEXT_CHARS),
    })
}

pub fn compose(asked: &Asked, missing: &[String], catalog: &[String]) -> Vec<Mapping> {
    if asked.failed() {
        return Vec::new();
    }
    missing
        .iter()
        .enumerate()
        .filter_map(|(index, typed)| {
            let (chosen, probability) = asked.choice(&key(index))?;
            let known = catalog.iter().any(|t| t == chosen);
            (known && probability >= MIN_MAP).then(|| Mapping {
                typed: typed.clone(),
                mapped: chosen.to_owned(),
                probability,
            })
        })
        .collect()
}

async fn catalog(persist: &PersistHandle, project_path: &str) -> Vec<String> {
    let Ok(Some(project_id)) = persist.project_id_for_path(project_path).await else {
        return Vec::new();
    };
    let mut topics = persist
        .lessons()
        .project_topics(project_id)
        .await
        .unwrap_or_default();
    topics.truncate(MAX_CATALOG);
    topics
}

pub async fn map_near_misses(
    env: Option<&DecisionEnv>,
    persist: &PersistHandle,
    project_path: &str,
    missing: &[String],
    context: &str,
) -> Vec<Mapping> {
    let Some(decider) = env.and_then(|e| e.decider(POINT)) else {
        return Vec::new();
    };
    let missing: Vec<String> = missing.iter().take(MAX_TYPED).cloned().collect();
    if missing.is_empty() {
        return Vec::new();
    }
    let catalog = catalog(persist, project_path).await;
    if catalog.is_empty() {
        return Vec::new();
    }
    let asked = decider
        .ask(
            QUESTION_SET,
            state(&missing, &catalog, context),
            questions(&missing, &catalog),
            DecisionSubject::default(),
            json!({ "min_map": MIN_MAP }),
        )
        .await;
    let mappings = compose(&asked, &missing, &catalog);
    let score = mappings.iter().map(|m| m.probability).fold(0.0, f64::max);
    let applied = if mappings.is_empty() {
        "none"
    } else {
        "mapped"
    };
    decider
        .mark(&asked, asked.composed(json!({ "score": score })), applied)
        .await;
    if decider.applies() {
        mappings
    } else {
        Vec::new()
    }
}

#[cfg(test)]
#[path = "topic_tests.rs"]
mod tests;
