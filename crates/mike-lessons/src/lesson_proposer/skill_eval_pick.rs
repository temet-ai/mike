use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};

use super::trigger::{Choice, Offer};
use crate::llm::decisions::env::{DecisionEnv, head_chars};
use crate::llm::decisions::point::{Asked, Decider, choice};
use crate::llm::decisions::{Question, record::DecisionSubject};

pub const POINT: &str = "skill_trigger";
pub const QUESTION_SET: &str = "skill_trigger_v1";
pub const NONE_KEY: &str = "none";
pub const MIN_SKILL: f64 = 0.5;
const MESSAGE_CHARS: usize = 500;
const DESCRIPTION_CHARS: usize = 240;
const PROJECT_CHARS: usize = 400;

pub struct JevTrigger {
    decider: Decider,
    tripped: AtomicBool,
}

impl JevTrigger {
    pub fn new(env: Option<&DecisionEnv>) -> Option<Self> {
        Some(Self {
            decider: env?.decider(POINT)?,
            tripped: AtomicBool::new(false),
        })
    }
}

fn question_key(number: usize) -> String {
    format!("message_{number}")
}

pub fn questions(entries: &[(String, String)], count: usize) -> BTreeMap<String, Question> {
    let mut options: Vec<(&str, String)> = entries
        .iter()
        .map(|(name, description)| {
            (
                name.as_str(),
                head_chars(description.trim(), DESCRIPTION_CHARS),
            )
        })
        .collect();
    options.push((
        NONE_KEY,
        "no skill clearly matches the work; general knowledge is enough".to_owned(),
    ));
    let refs: Vec<(&str, &str)> = options.iter().map(|(k, v)| (*k, v.as_str())).collect();
    (1..=count)
        .map(|number| {
            (
                question_key(number),
                choice(
                    &format!(
                        "Which skill, if any, would you load before starting work on message {number}? A skill that only shares words with the message is not a match."
                    ),
                    &refs,
                ),
            )
        })
        .collect()
}

pub fn state(project: &str, messages: &[&str]) -> Value {
    json!({
        "project": head_chars(project.trim(), PROJECT_CHARS),
        "messages": messages
            .iter()
            .zip(1_usize..)
            .map(|(text, number)| json!({ "number": number, "text": head_chars(text.trim(), MESSAGE_CHARS) }))
            .collect::<Vec<_>>(),
    })
}

pub fn compose(asked: &Asked, count: usize, entries: &[(String, String)]) -> Option<Vec<Choice>> {
    if asked.failed() {
        return None;
    }
    (1..=count)
        .map(|number| {
            let (key, p) = asked.choice(&question_key(number))?;
            let known = entries.iter().any(|(name, _)| name == key);
            let load = if known && p >= MIN_SKILL {
                vec![key.to_owned()]
            } else {
                Vec::new()
            };
            Some(Choice {
                message: i64::try_from(number).ok()?,
                load,
            })
        })
        .collect()
}

fn mean_score(asked: &Asked, count: usize) -> f64 {
    let scores: Vec<f64> = (1..=count)
        .filter_map(|n| asked.choice(&question_key(n)))
        .map(|(_, p)| p)
        .collect();
    if scores.is_empty() {
        return 0.0;
    }
    scores.iter().sum::<f64>() / scores.len() as f64
}

pub async fn ask_batch(
    jev: &JevTrigger,
    offer: &Offer<'_>,
    messages: &[&str],
) -> Option<Vec<Choice>> {
    if jev.tripped.load(Ordering::Relaxed) {
        return None;
    }
    let asked = jev
        .decider
        .ask(
            QUESTION_SET,
            state(offer.project, messages),
            questions(offer.entries, messages.len()),
            DecisionSubject::default(),
            json!({ "min_skill": MIN_SKILL }),
        )
        .await;
    if asked.failed() {
        jev.tripped.store(true, Ordering::Relaxed);
    }
    let composed = compose(&asked, messages.len(), offer.entries);
    let applied = match &composed {
        Some(c) if c.iter().any(|m| !m.load.is_empty()) => "chosen",
        _ => "none",
    };
    jev.decider
        .mark(
            &asked,
            asked.composed(json!({ "score": mean_score(&asked, messages.len()) })),
            applied,
        )
        .await;
    composed.filter(|_| jev.decider.applies())
}

#[cfg(test)]
#[path = "skill_eval_pick_tests.rs"]
mod tests;
