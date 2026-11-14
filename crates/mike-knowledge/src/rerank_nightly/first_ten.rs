use serde::Serialize;
use serde_json::Value;

use super::grade::rerank_all;
use super::search_decide::{self, SearchJev};
use crate::search_ranking::tuning::GradeCandidate;
use crate::semantic::KnowledgeState;

pub const FIRST_TEN: usize = 10;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResultGrade {
    pub reference: String,
    pub position: usize,
    pub query_score: Option<f64>,
    pub turn_score: Option<f64>,
    pub relevant: Option<bool>,
}

pub struct Graded {
    pub results: Vec<ResultGrade>,
    pub relevant: Vec<String>,
}

pub struct Scored<'a> {
    pub candidates: &'a [GradeCandidate],
    pub documents: &'a [String],
    pub jev: Option<&'a SearchJev>,
    pub query: &'a str,
    pub turn_prompt: Option<&'a str>,
    pub used: &'a [String],
}

pub fn first_ten_references(returned: &Value) -> Vec<String> {
    returned
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["reference"].as_str().map(str::to_owned))
                .take(FIRST_TEN)
                .collect()
        })
        .unwrap_or_default()
}

pub fn turn_query(query: &str, turn_prompt: Option<&str>) -> Option<String> {
    let prompt = turn_prompt.map(str::trim).filter(|p| !p.is_empty())?;
    Some(format!("{query}\n\nAsked in the turn: {prompt}"))
}

pub async fn grade_first_ten(
    state: &KnowledgeState,
    references: &[String],
    turn_query: Option<&str>,
    scored: &Scored<'_>,
) -> Graded {
    let found: Vec<Option<usize>> = references
        .iter()
        .map(|reference| {
            scored
                .candidates
                .iter()
                .position(|c| &c.reference == reference)
        })
        .collect();
    let turn_scores = turn_scores(state, turn_query, &found, scored.documents).await;
    let threshold = state.rerank_service().current_config().rescue_score;
    let mut results: Vec<ResultGrade> = references
        .iter()
        .zip(found.iter().zip(turn_scores))
        .enumerate()
        .map(|(index, (reference, (at, turn_score)))| {
            let query_score = at
                .and_then(|i| scored.candidates.get(i))
                .and_then(|c| c.rerank_score);
            ResultGrade {
                reference: reference.clone(),
                position: index + 1,
                query_score,
                turn_score,
                relevant: relevance(query_score, turn_score, threshold),
            }
        })
        .collect();
    if let Some(jev) = scored.jev {
        let texts: Vec<Option<&str>> = found
            .iter()
            .map(|at| at.and_then(|i| scored.documents.get(i)).map(String::as_str))
            .collect();
        search_decide::grade(
            jev,
            scored.query,
            scored.turn_prompt,
            &mut results,
            &texts,
            scored.used,
        )
        .await;
    }
    let relevant = results
        .iter()
        .filter(|r| r.relevant == Some(true))
        .map(|r| r.reference.clone())
        .collect();
    Graded { results, relevant }
}

async fn turn_scores(
    state: &KnowledgeState,
    turn_query: Option<&str>,
    found: &[Option<usize>],
    documents: &[String],
) -> Vec<Option<f64>> {
    let none = vec![None; found.len()];
    let Some(query) = turn_query else {
        return none;
    };
    let slots: Vec<(usize, String)> = found
        .iter()
        .enumerate()
        .filter_map(|(slot, at)| at.and_then(|i| documents.get(i)).map(|d| (slot, d.clone())))
        .collect();
    let texts: Vec<String> = slots.iter().map(|(_, text)| text.clone()).collect();
    if texts.is_empty() {
        return none;
    }
    let Some(scores) = rerank_all(state, query, &texts).await else {
        return none;
    };
    let mut out = none;
    for ((slot, _), score) in slots.iter().zip(scores) {
        if let Some(entry) = out.get_mut(*slot) {
            *entry = score;
        }
    }
    out
}

pub fn relevance(
    query_score: Option<f64>,
    turn_score: Option<f64>,
    threshold: f64,
) -> Option<bool> {
    match (query_score, turn_score) {
        (None, None) => None,
        (q, t) => Some(q.is_some_and(|s| s >= threshold) || t.is_some_and(|s| s >= threshold)),
    }
}
