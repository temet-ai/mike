use std::collections::HashSet;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::progress::in_chunks;
use super::{EvalError, TriggerQuery};
use crate::llm::OpenRouterClient;
use crate::store::persistence::lessons::skill_draft_eval::SiblingSkill;

pub const BATCH_SIZE: usize = 10;

pub use crate::settings::memory_prompts::SKILL_TRIGGER_PROMPT as PROMPT;

#[derive(Debug, Deserialize)]
struct Answers {
    answers: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
pub struct Choice {
    pub message: i64,
    pub load: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryResult {
    pub query: String,
    pub should_trigger: bool,
    pub loaded: Vec<String>,
    pub triggered: bool,
    pub correct: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TriggerEval {
    pub correct: usize,
    pub total: usize,
    pub score: f64,
    pub results: Vec<QueryResult>,
}

pub struct Offer<'a> {
    pub name: &'a str,
    pub entries: &'a [(String, String)],
    pub project: &'a str,
}

fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "answers": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "message": {"type": "integer"},
                        "load": {"type": "array", "items": {"type": "string"}}
                    },
                    "required": ["message", "load"]
                }
            }
        },
        "required": ["answers"]
    })
}

pub fn batch_count(total: usize) -> usize {
    total.div_ceil(BATCH_SIZE)
}

pub fn batch_indexes(queries: &[TriggerQuery], batch: usize) -> Vec<usize> {
    let stride = batch_count(queries.len()).max(1);
    let labelled = |wanted: bool| {
        queries
            .iter()
            .enumerate()
            .filter(move |(_, q)| q.should_trigger == wanted)
            .map(|(index, _)| index)
    };
    let mut picked: Vec<usize> = labelled(true)
        .chain(labelled(false))
        .enumerate()
        .filter(|(dealt, _)| dealt % stride == batch)
        .map(|(_, index)| index)
        .collect();
    picked.sort_unstable();
    picked
}

pub fn listing(entries: &[(String, String)], rotate: usize) -> String {
    let mut out = String::new();
    for offset in 0..entries.len() {
        if let Some((name, description)) = entries.get((rotate + offset) % entries.len()) {
            let _ = writeln!(out, "- {name}: {}", description.trim());
        }
    }
    out
}

pub fn triggered(loaded: &[String], name: &str) -> bool {
    loaded.iter().any(|l| l.trim() == name)
}

pub fn score(results: Vec<QueryResult>) -> TriggerEval {
    let total = results.len();
    let correct = results.iter().filter(|r| r.correct).count();
    let score = if total == 0 {
        0.0
    } else {
        correct as f64 / total as f64
    };
    TriggerEval {
        correct,
        total,
        score,
        results,
    }
}

pub fn batch_input(entries: &[(String, String)], rotate: usize, messages: &[&str]) -> String {
    let mut out = format!(
        "## AVAILABLE SKILLS\n{}\n## USER MESSAGES\n",
        listing(entries, rotate)
    );
    for (number, message) in (1..).zip(messages) {
        let _ = write!(out, "{number}. {}\n\n", message.trim());
    }
    out
}

pub fn offer_input(offer: &Offer<'_>, batch: usize, messages: &[&str]) -> String {
    let mut input = String::new();
    if !offer.project.trim().is_empty() {
        let _ = write!(input, "## THE PROJECT\n{}\n\n", offer.project.trim());
    }
    input.push_str(&batch_input(offer.entries, batch, messages));
    input
}

pub fn matched(
    queries: &[&TriggerQuery],
    mut answers: Vec<Choice>,
    name: &str,
) -> Result<Vec<QueryResult>, EvalError> {
    answers.sort_by_key(|a| a.message);
    let numbered = answers
        .iter()
        .zip(1_i64..)
        .all(|(answer, number)| answer.message == number);
    if answers.len() != queries.len() || !numbered {
        return Err(EvalError::Model(format!(
            "trigger test answered {} messages numbered {:?} for {} messages",
            answers.len(),
            answers.iter().map(|a| a.message).collect::<Vec<_>>(),
            queries.len()
        )));
    }
    Ok(queries
        .iter()
        .zip(answers)
        .map(|(query, answer)| {
            let loaded_it = triggered(&answer.load, name);
            QueryResult {
                query: query.query.clone(),
                should_trigger: query.should_trigger,
                loaded: answer.load,
                triggered: loaded_it,
                correct: loaded_it == query.should_trigger,
            }
        })
        .collect())
}

async fn ask_batch(
    llm: &OpenRouterClient,
    prompt: &str,
    offer: &Offer<'_>,
    batch: usize,
    queries: &[&TriggerQuery],
) -> Result<Vec<QueryResult>, EvalError> {
    let messages: Vec<&str> = queries.iter().map(|q| q.query.as_str()).collect();
    let input = offer_input(offer, batch, &messages);
    let reply: Answers = llm.verdict(prompt, &input, schema()).await?;
    matched(queries, reply.answers, offer.name)
}

pub fn entries(
    name: &str,
    description: &str,
    siblings: &[SiblingSkill],
    real: &[SiblingSkill],
) -> Vec<(String, String)> {
    let mut seen = HashSet::new();
    seen.insert(name.to_owned());
    let mut entries = vec![(name.to_owned(), description.to_owned())];
    for skill in siblings.iter().chain(real) {
        if seen.insert(skill.folder.clone()) {
            entries.push((skill.folder.clone(), skill.description.clone()));
        }
    }
    entries
}

pub async fn run_batches(
    llm: &OpenRouterClient,
    prompt: &str,
    offer: &Offer<'_>,
    queries: &[TriggerQuery],
    batches: &[usize],
) -> Vec<Result<Vec<QueryResult>, EvalError>> {
    let tasks: Vec<_> = batches
        .iter()
        .map(|&batch| {
            let picked: Vec<&TriggerQuery> = batch_indexes(queries, batch)
                .into_iter()
                .filter_map(|index| queries.get(index))
                .collect();
            async move { ask_batch(llm, prompt, offer, batch, &picked).await }
        })
        .collect();
    in_chunks(tasks).await
}
