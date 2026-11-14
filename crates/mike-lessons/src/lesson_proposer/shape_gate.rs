use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::ProposerDeps;
use crate::lesson_shape::{Assessment, Shape, opens_with_status_word};
use crate::llm::decisions::env::{Pace, head_chars};
use crate::llm::decisions::point::{Asked, noul};
use crate::llm::decisions::{Question, record::DecisionSubject};
use crate::store::persistence::lessons::shape::UncheckedLesson;

pub const POINT: &str = "lesson_shape";
pub const QUESTION_SET: &str = "lesson_shape_v1";
pub const MIN_RULE: f64 = 0.7;
const GROUP: usize = 5;
const MAX_CALLS: usize = 4;
const TEXT_CHARS: usize = 700;
const VERIFIER_TOPIC_PREFIX: &str = "verifier:";

fn question_key(id: i64) -> String {
    format!("lesson_{id}")
}

pub fn candidates<'a>(
    rows: &'a [UncheckedLesson],
    findings: &[(i64, Assessment)],
) -> Vec<&'a UncheckedLesson> {
    rows.iter()
        .filter(|row| {
            let flagged_status = findings
                .iter()
                .any(|(id, a)| *id == row.id && a.shape == Shape::Status);
            let verifier = row
                .topic
                .as_deref()
                .is_some_and(|t| t.starts_with(VERIFIER_TOPIC_PREFIX));
            flagged_status && !verifier && opens_with_status_word(&row.text)
        })
        .collect()
}

pub fn questions(group: &[&UncheckedLesson]) -> BTreeMap<String, Question> {
    group
        .iter()
        .map(|row| {
            (
                question_key(row.id),
                noul(
                    &format!(
                        "Does lesson {} state a rule or instruction a later session can act on, as opposed to reporting what was done or what state something is in?",
                        row.id
                    ),
                    "the text states a rule, a definition or an instruction a later session can follow",
                    "the text only reports an outcome or a status",
                ),
            )
        })
        .collect()
}

pub fn state(group: &[&UncheckedLesson]) -> Value {
    json!({
        "lessons": group
            .iter()
            .map(|row| json!({
                "id": row.id,
                "topic": row.topic,
                "text": head_chars(&row.text, TEXT_CHARS),
                "opens_with_status_word": true,
            }))
            .collect::<Vec<_>>(),
    })
}

pub fn compose(asked: &Asked, group: &[&UncheckedLesson]) -> Vec<(i64, f64)> {
    if asked.failed() {
        return Vec::new();
    }
    group
        .iter()
        .filter_map(|row| {
            let p = asked.noul(&question_key(row.id))?;
            (p >= MIN_RULE).then_some((row.id, p))
        })
        .collect()
}

pub fn applied_score(asked: &Asked, group: &[&UncheckedLesson]) -> f64 {
    let scores: Vec<f64> = group
        .iter()
        .filter_map(|row| asked.noul(&question_key(row.id)))
        .map(|p| if p >= MIN_RULE { p } else { 1.0 - p })
        .collect();
    if scores.is_empty() {
        return 0.0;
    }
    scores.iter().sum::<f64>() / scores.len() as f64
}

fn cleared(assessment: &Assessment, p: f64) -> Assessment {
    Assessment {
        shape: Shape::Rule,
        detail: format!("a rule; Jev judged that it states one (probability {p:.2})"),
        chars: assessment.chars,
    }
}

pub async fn clear_false_status(
    deps: &ProposerDeps<'_>,
    rows: &[UncheckedLesson],
    findings: &mut [(i64, Assessment)],
) {
    let Some(decider) = deps.decisions.and_then(|e| e.decider(POINT)) else {
        return;
    };
    let wanted = candidates(rows, findings);
    let mut pace = Pace::new(MAX_CALLS);
    for group in wanted.chunks(GROUP) {
        if !pace.may_ask() {
            break;
        }
        let asked = decider
            .ask(
                QUESTION_SET,
                state(group),
                questions(group),
                DecisionSubject::default(),
                json!({ "min_rule": MIN_RULE }),
            )
            .await;
        pace.spent(&asked);
        let rules = compose(&asked, group);
        let score = applied_score(&asked, group);
        let applied = if rules.is_empty() { "none" } else { "chosen" };
        decider
            .mark(&asked, asked.composed(json!({ "score": score })), applied)
            .await;
        if !decider.applies() {
            continue;
        }
        for (id, p) in rules {
            if let Some((_, assessment)) = findings.iter_mut().find(|(fid, _)| *fid == id) {
                *assessment = cleared(assessment, p);
            }
        }
    }
}

#[cfg(test)]
#[path = "shape_gate_tests.rs"]
mod tests;
