use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::judge::{CriterionText, JudgeAnswer, JudgeInput, JudgeVerdict, Judged};
use crate::llm::decisions::Question;
use crate::llm::decisions::point::{Asked, Decider, noul};
use crate::llm::decisions::record::DecisionSubject;
use crate::llm::decisions::text::{head_chars, tail_chars};

pub const POINT: &str = "leaf_judge";
pub const QUESTION_SET: &str = "leaf_judge_v1";
pub const PASS_AT: f64 = 0.75;
pub const FAIL_AT: f64 = 0.25;
const DIFF_HEAD_CHARS: usize = 4000;
const DIFF_TAIL_CHARS: usize = 2000;

#[derive(Debug, Clone, PartialEq)]
pub enum Composed {
    Pass { weakest: f64 },
    Fail { unmet: Vec<(String, f64)> },
    Check,
}

fn key(criterion: &CriterionText) -> String {
    format!("criterion_{}", criterion.id)
}

pub fn questions(criteria: &[CriterionText]) -> BTreeMap<String, Question> {
    criteria
        .iter()
        .map(|criterion| {
            (
                key(criterion),
                noul(
                    &format!(
                        "A fleet worker reported its task done. Does the evidence (the report, the exercised command and its exit code, the changed tests and the diff) show that criterion {} is met: {}",
                        criterion.id, criterion.text
                    ),
                    "The evidence shows the criterion is met.",
                    "The evidence does not show the criterion is met, or shows it is not.",
                ),
            )
        })
        .collect()
}

fn diff_excerpt(diff: &str) -> String {
    if diff.chars().count() <= DIFF_HEAD_CHARS + DIFF_TAIL_CHARS {
        return diff.to_owned();
    }
    format!(
        "{}\n... diff cut here ...\n{}",
        head_chars(diff, DIFF_HEAD_CHARS),
        tail_chars(diff, DIFF_TAIL_CHARS)
    )
}

pub fn state(input: &JudgeInput) -> Value {
    json!({
        "task": input.task,
        "criteria": input.criteria,
        "report": input.report,
        "floor_passed_at": input.floor.sha(),
        "diff": diff_excerpt(&input.diff),
        "open_obligations": input.open_obligations,
        "round": input.earlier_verdicts.len() + 1,
        "earlier_verdicts": input.earlier_verdicts,
    })
}

fn establishes_nothing_new(input: &JudgeInput) -> bool {
    input.report.exercised_exit != Some(0) || !input.report.unestablished.is_empty()
}

pub fn compose(asked: &Asked, input: &JudgeInput) -> Composed {
    if asked.failed() || input.criteria.is_empty() {
        return Composed::Check;
    }
    let mut answers = Vec::with_capacity(input.criteria.len());
    for criterion in &input.criteria {
        let Some(p) = asked.noul(&key(criterion)) else {
            return Composed::Check;
        };
        answers.push((criterion.id.clone(), p));
    }
    let unmet: Vec<(String, f64)> = answers
        .iter()
        .filter(|(_, p)| *p <= FAIL_AT)
        .cloned()
        .collect();
    if !unmet.is_empty() {
        return Composed::Fail { unmet };
    }
    let weakest = answers.iter().map(|(_, p)| *p).fold(1.0, f64::min);
    if weakest >= PASS_AT && !establishes_nothing_new(input) {
        return Composed::Pass { weakest };
    }
    Composed::Check
}

fn evidence_read(input: &JudgeInput) -> Vec<String> {
    vec![
        format!(
            "report: exercised `{}` (exit {:?}), changed tests {:?}",
            input.report.exercised_command, input.report.exercised_exit, input.report.tests_changed
        ),
        format!("the diff of commit {}", input.floor.sha()),
    ]
}

pub fn answer_of(composed: &Composed, input: &JudgeInput) -> Option<JudgeAnswer> {
    let (verdict, criteria, needed, observation) = match composed {
        Composed::Check => return None,
        Composed::Pass { weakest } => (
            JudgeVerdict::Pass,
            input.criteria.iter().map(|c| c.id.clone()).collect(),
            None,
            format!(
                "Jev found every criterion shown met by the report and the diff; the least certain answer was {:.0}% yes.",
                weakest * 100.0
            ),
        ),
        Composed::Fail { unmet } => (
            JudgeVerdict::Fail,
            unmet.iter().map(|(id, _)| id.clone()).collect(),
            Some(unmet_text(unmet, input)),
            format!(
                "Jev found {} criterion(s) not shown met by the report and the diff.",
                unmet.len()
            ),
        ),
    };
    Some(JudgeAnswer {
        verdict,
        criteria,
        sha: input.floor.sha().to_owned(),
        evidence_read: evidence_read(input),
        needed,
        observation,
        checks: Vec::new(),
    })
}

fn unmet_text(unmet: &[(String, f64)], input: &JudgeInput) -> String {
    unmet
        .iter()
        .map(|(id, p)| {
            let text = input
                .criteria
                .iter()
                .find(|c| &c.id == id)
                .map_or("", |c| c.text.as_str());
            format!(
                "Criterion {id} ({text}) is not shown met by the report and the diff ({:.0}% yes); show it with a check that exercises it.",
                p * 100.0
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub async fn jev_judge(decider: &Decider, input: &JudgeInput, note: uuid::Uuid) -> Option<Judged> {
    if !input.floor.reaches_judge() {
        return None;
    }
    let asked = decider
        .ask(
            QUESTION_SET,
            state(input),
            questions(&input.criteria),
            DecisionSubject {
                commit_sha: Some(input.floor.sha().to_owned()),
                message_uuid: Some(note),
                ..DecisionSubject::default()
            },
            json!({"pass_at": PASS_AT, "fail_at": FAIL_AT}),
        )
        .await;
    let composed = compose(&asked, input);
    let answer = answer_of(&composed, input).filter(|_| decider.applies());
    let score = match &composed {
        Composed::Pass { weakest } => *weakest,
        Composed::Fail { unmet } => unmet.iter().map(|(_, p)| 1.0 - p).fold(1.0, f64::min),
        Composed::Check => 0.0,
    };
    let applied = if answer.is_some() { "chosen" } else { "none" };
    let verdict = match &composed {
        Composed::Pass { .. } => Some("pass"),
        Composed::Fail { .. } => Some("fail"),
        Composed::Check => None,
    };
    decider
        .mark(
            &asked,
            asked.composed(json!({"score": score, "verdict": verdict})),
            applied,
        )
        .await;
    answer.map(|answer| Judged {
        answer,
        checked: Vec::new(),
    })
}

#[cfg(test)]
#[path = "judge_jev_tests.rs"]
mod tests;
