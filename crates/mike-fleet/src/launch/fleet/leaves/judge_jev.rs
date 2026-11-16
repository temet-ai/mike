use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::clauses::{clause_questions, every_clause_shown};
use super::judge::{CriterionText, JudgeAnswer, JudgeInput, JudgeVerdict, Judged, ReportFacts};
use crate::llm::decisions::Question;
use crate::llm::decisions::point::{Asked, Decider, noul};
use crate::llm::decisions::record::DecisionSubject;
use crate::llm::decisions::text::{head_chars, tail_chars};

pub const POINT: &str = "leaf_judge";
pub const QUESTION_SET: &str = "leaf_judge_v4";
pub const PASS_AT: f64 = 0.75;
pub const FAIL_AT: f64 = 0.25;
pub const LEANS_MET_AT: f64 = 0.5;
pub const RUNS_CHECK_AT: f64 = 0.75;
pub const CAVEAT_AT: f64 = 0.5;
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

fn runs_check_key(criterion: &CriterionText) -> String {
    format!("runs_check_{}", criterion.id)
}

fn caveat_key(criterion: &CriterionText) -> String {
    format!("caveat_{}", criterion.id)
}

fn met_question(criterion: &CriterionText) -> Question {
    noul(
        &format!(
            "A fleet worker reported its task done. Does the evidence (the report, the exercised command with its exit code and the end of its output in `report.exercised_output`, the test cases in the files that check runs in `report.check_tests`, the changed tests and the diff) show that this task's part of criterion {} is met, judged by task_brief, while the parts given to the tasks in shared_with are theirs to show: {}",
            criterion.id, criterion.text
        ),
        "The evidence shows the criterion is met.",
        "The evidence does not show the criterion is met, or shows it is not.",
    )
}

fn runs_check_question(criterion: &CriterionText) -> Question {
    noul(
        &format!(
            "Does `report.exercised_command` run the check that criterion {} names in its `check` field, the same command or one that covers it: {}",
            criterion.id, criterion.check
        ),
        "The exercised command is that check, or runs it with the same test files or targets.",
        "The exercised command is some other command, or the criterion names no check it runs.",
    )
}

fn caveat_question(criterion: &CriterionText) -> Question {
    noul(
        &format!(
            "Does an item in `report.unestablished` say that something criterion {} requires is not shown? The criterion: {}",
            criterion.id, criterion.text
        ),
        "An unestablished item names a part of this criterion that the evidence does not show.",
        "Every unestablished item is a caveat this criterion does not require, such as an estimate its check allows or a live observation it does not ask for.",
    )
}

pub fn questions(criteria: &[CriterionText], report: &ReportFacts) -> BTreeMap<String, Question> {
    let mut out = BTreeMap::new();
    for criterion in criteria {
        out.insert(key(criterion), met_question(criterion));
        out.insert(runs_check_key(criterion), runs_check_question(criterion));
        out.extend(clause_questions(criterion));
        if !report.unestablished.is_empty() {
            out.insert(caveat_key(criterion), caveat_question(criterion));
        }
    }
    out
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
        "diff": diff_excerpt(&input.diff),
        "open_obligations": input.open_obligations,
        "task_brief": input.task_brief,
        "shared_with": input.shared_with,
    })
}

fn caveat_blocks(asked: &Asked, criterion: &CriterionText, report: &ReportFacts) -> bool {
    !report.unestablished.is_empty()
        && asked
            .noul(&caveat_key(criterion))
            .is_none_or(|p| p >= CAVEAT_AT)
}

fn shown_met(asked: &Asked, criterion: &CriterionText, met: f64) -> bool {
    let runs_check = asked.noul(&runs_check_key(criterion)).unwrap_or(0.0);
    met >= PASS_AT
        || (met >= LEANS_MET_AT && runs_check >= RUNS_CHECK_AT)
        || every_clause_shown(asked, criterion)
}

pub fn decide(asked: &Asked, criteria: &[CriterionText], report: &ReportFacts) -> Composed {
    if asked.failed() || criteria.is_empty() {
        return Composed::Check;
    }
    let mut answers = Vec::with_capacity(criteria.len());
    for criterion in criteria {
        let Some(p) = asked.noul(&key(criterion)) else {
            return Composed::Check;
        };
        answers.push((criterion, p));
    }
    let unmet: Vec<(String, f64)> = answers
        .iter()
        .filter(|(c, p)| *p <= FAIL_AT && !every_clause_shown(asked, c))
        .map(|(c, p)| (c.id.clone(), *p))
        .collect();
    if !unmet.is_empty() {
        return Composed::Fail { unmet };
    }
    let all_shown = report.exercised_exit == Some(0)
        && answers
            .iter()
            .all(|(c, p)| shown_met(asked, c, *p) && !caveat_blocks(asked, c, report));
    if all_shown {
        let weakest = answers.iter().map(|(_, p)| *p).fold(1.0, f64::min);
        return Composed::Pass { weakest };
    }
    Composed::Check
}

pub fn compose(asked: &Asked, input: &JudgeInput) -> Composed {
    decide(asked, &input.criteria, &input.report)
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
            questions(&input.criteria, &input.report),
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
