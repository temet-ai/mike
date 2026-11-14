use std::collections::BTreeMap;

use chrono::Utc;
use serde_json::{Value, json};

use super::jev::{Jev, Reply};
use super::point::{Judged, ReplayPoint};
use crate::llm::decisions::point::Asked;
use crate::llm::decisions::{Answer, DecisionRequest};

pub(in crate::config::subcommands) const SCHEMA: &str = "decisions_replay_v1";
const YES: f64 = 0.5;
const FAILURES_IN_A_ROW: usize = 10;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Sample {
    pub score: f64,
    pub positive: Option<bool>,
    pub label_right: Option<bool>,
    pub agrees_current: Option<bool>,
}

pub(super) struct Outcome {
    pub line: Value,
    pub failed: bool,
    pub questions: Vec<(String, Sample)>,
    pub composed: Option<Sample>,
}

pub(super) struct Replayed {
    pub outcomes: Vec<Outcome>,
    pub stopped: Option<String>,
}

pub(super) async fn replay<P: ReplayPoint>(items: &[P::Item], jev: &impl Jev) -> Replayed {
    let mut outcomes = Vec::with_capacity(items.len());
    let mut failures = 0;
    for item in items {
        let prepared = P::prepare(item);
        let request = DecisionRequest {
            model: jev.model().to_owned(),
            state: prepared.state,
            questions: prepared.questions,
        };
        let reply = match jev.ask(&request).await {
            Reply::Stop(reason) => {
                return Replayed {
                    outcomes,
                    stopped: Some(reason),
                };
            }
            reply => reply,
        };
        let outcome = outcome::<P>(item, &request, reply);
        failures = if outcome.failed { failures + 1 } else { 0 };
        outcomes.push(outcome);
        if failures >= FAILURES_IN_A_ROW {
            return Replayed {
                outcomes,
                stopped: Some(format!("{FAILURES_IN_A_ROW} calls in a row failed")),
            };
        }
    }
    Replayed {
        outcomes,
        stopped: None,
    }
}

fn header<P: ReplayPoint>(item: &P::Item, request: &DecisionRequest) -> Value {
    let id = P::item_id(item);
    json!({
        "schema": SCHEMA,
        "point": P::POINT,
        "question_set": P::QUESTION_SET,
        "item_id": id,
        "source_ref": format!("decisions_replay:{}:{}:{id}", P::POINT, P::QUESTION_SET),
        "item_at": P::item_at(item),
        "asked_at": Utc::now(),
        "label": P::label(item),
        "request": request,
        "answers": null,
        "error": null,
        "model": null,
        "latency_ms": null,
        "cost_usd": null,
        "composed": null,
        "score": null,
        "outcome": null,
        "agrees_current": null,
    })
}

fn outcome<P: ReplayPoint>(item: &P::Item, request: &DecisionRequest, reply: Reply) -> Outcome {
    let mut line = header::<P>(item, request);
    match reply {
        Reply::Answered {
            model,
            answers,
            cost_usd,
            latency_ms,
        } => {
            line["model"] = json!(model);
            line["cost_usd"] = json!(cost_usd);
            line["latency_ms"] = json!(latency_ms);
            answered::<P>(item, request, line, answers)
        }
        Reply::Failed(error) | Reply::Stop(error) => {
            line["error"] = json!(error);
            Outcome {
                line,
                failed: true,
                questions: Vec::new(),
                composed: None,
            }
        }
    }
}

fn question_samples<P: ReplayPoint>(
    item: &P::Item,
    request: &DecisionRequest,
    asked: &Asked,
) -> Vec<(String, Sample)> {
    request
        .questions
        .keys()
        .filter_map(|question| {
            let score = asked.noul(question)?;
            let positive = P::question_positive(item, question);
            let current = P::current_positive(item, question);
            Some((
                question.clone(),
                Sample {
                    score,
                    positive,
                    label_right: positive.map(|p| (score >= YES) == p),
                    agrees_current: current.map(|c| (score >= YES) == c),
                },
            ))
        })
        .collect()
}

fn composed_sample(judged: &Judged) -> Sample {
    Sample {
        score: judged.score,
        positive: judged.positive,
        label_right: judged.decision_right,
        agrees_current: judged.agrees_current,
    }
}

fn answered<P: ReplayPoint>(
    item: &P::Item,
    request: &DecisionRequest,
    mut line: Value,
    answers: BTreeMap<String, Answer>,
) -> Outcome {
    let asked = Asked::answered(answers.clone());
    line["answers"] = json!(answers);
    let questions = question_samples::<P>(item, request, &asked);
    let judged = P::compose(item, &asked);
    if let Some(judged) = &judged {
        line["composed"] = asked.composed(judged.composed.clone());
        line["score"] = json!(judged.score);
        line["agrees_current"] = json!(judged.agrees_current);
        line["outcome"] = judged.decision_right.map_or(Value::Null, |right| {
            json!({
                "label": if right { "right" } else { "wrong" },
                "source": P::OUTCOME_SOURCE,
            })
        });
    }
    Outcome {
        line,
        failed: asked.failed(),
        questions,
        composed: judged.as_ref().map(composed_sample),
    }
}
