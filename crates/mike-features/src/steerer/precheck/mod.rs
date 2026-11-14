pub mod compose;
pub mod questions;
pub mod score;
pub mod state;

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::json;

use crate::llm::OpenRouterClient;
use crate::llm::decisions::DecisionRequest;
use crate::llm::decisions::record::DecisionContext;
use crate::steerer::guard::Question;
use compose::{Verdict, compose};
use state::{PrecheckInput, state};

pub const POINT: &str = "turn_guard_precheck";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Precheck {
    pub record_id: Option<i64>,
    pub question_set: &'static str,
    pub answers: BTreeMap<String, f64>,
    pub refusals: usize,
    pub verdict: Verdict,
    pub fail_open: bool,
    pub error: Option<String>,
}

pub fn applies_to(question: Question) -> bool {
    questions::questions_for(question).is_some()
}

fn failed_open(record_id: Option<i64>, error: String) -> Precheck {
    Precheck {
        record_id,
        question_set: questions::QUESTION_SET,
        answers: BTreeMap::new(),
        refusals: 0,
        verdict: Verdict {
            send: true,
            score: 1.0,
            why: format!("the pre-check failed, so the fire is sent: {error}"),
        },
        fail_open: true,
        error: Some(error),
    }
}

pub async fn run(
    client: &OpenRouterClient,
    input: &PrecheckInput<'_>,
    mut context: DecisionContext,
    hold_below: f64,
) -> Option<Precheck> {
    let asked = questions::questions_for(input.question)?;
    context.thresholds = json!({ "hold_below": hold_below });
    let request = DecisionRequest {
        model: client.model().to_owned(),
        state: state(input),
        questions: asked,
    };
    let outcome = client.decide(&request, &context).await;
    let response = match outcome.result {
        Ok(response) => response,
        Err(error) => return Some(failed_open(outcome.record_id, error.to_string())),
    };
    let answers: BTreeMap<String, f64> = response
        .answers
        .iter()
        .filter_map(|(k, a)| a.noul().map(|p| (k.clone(), p)))
        .collect();
    let verdict = compose(input.question, &answers, hold_below);
    Some(Precheck {
        record_id: outcome.record_id,
        question_set: questions::QUESTION_SET,
        refusals: response.refusals(),
        fail_open: answers.len() < request.questions.len(),
        answers,
        verdict,
        error: None,
    })
}
