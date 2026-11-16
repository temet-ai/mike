use std::collections::BTreeMap;

use super::state::Criterion;
use crate::llm::decisions::Question;
use crate::llm::decisions::point::{choice, noul};

pub const POINT: &str = "steerer_checklist";
pub const QUESTION_SET: &str = "steerer_checklist_v3";
pub const CLAIMS_FINISHED: &str = "claims_finished";

pub const DONE: &str = "done";
pub const OPEN_STATED: &str = "open_stated";
pub const MISSING: &str = "missing";
pub const BROKEN: &str = "broken";
pub const NOT_SHOWN_BROKEN: &str = "not_shown_broken";

pub fn key(index: usize) -> String {
    format!("criterion_{index}")
}

fn outcome_question(index: usize) -> Question {
    let k = key(index);
    choice(
        &format!(
            "criteria.{k}.text is a goal for the whole task, not for this turn alone; criteria.{k}.their_words, when present, is what the operator said. Going by state.evidence (this turn's tool calls and the end of the turn) and state.facts (earlier_turns holds the ends of the session's previous turns; ends_with_operator_question is true when the turn ends by asking the operator something), which describes criteria.{k}?"
        ),
        &[
            (
                DONE,
                "The evidence shows it done: a tool call or its result in this turn, or an earlier turn's end, shows it.",
            ),
            (
                OPEN_STATED,
                "The turn says it is not finished yet: still in progress, running, waiting on a check, merge or deploy, or handed to the operator to decide.",
            ),
            (
                MISSING,
                "The turn claims it done with no tool call or result behind the claim, or does not mention it, and nothing shows it done.",
            ),
        ],
    )
}

fn rule_question(index: usize) -> Question {
    let k = key(index);
    choice(
        &format!(
            "criteria.{k}.text is a rule the work must keep, not a task to finish. Does state.evidence (this turn's tool calls and the end of the turn) show this turn breaking it?"
        ),
        &[
            (
                BROKEN,
                "A tool call or the turn's own text shows this turn doing what the rule forbids.",
            ),
            (
                NOT_SHOWN_BROKEN,
                "Nothing in the evidence shows the rule broken.",
            ),
        ],
    )
}

fn claims_finished_question() -> Question {
    noul(
        "Does `evidence.turn_end` tell the operator the whole task is finished, with nothing left to do?",
        "The turn reports the task as done, shipped or complete, such as \"All done: merged and deployed.\"",
        "The turn reports progress, a next step, a wait or a question, such as \"The push is running; I'll merge when it passes.\"",
    )
}

pub fn questions(criteria: &[Criterion]) -> BTreeMap<String, Question> {
    let mut set: BTreeMap<String, Question> = criteria
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let question = if c.is_rule() {
                rule_question(i)
            } else {
                outcome_question(i)
            };
            (key(i), question)
        })
        .collect();
    set.insert(CLAIMS_FINISHED.to_owned(), claims_finished_question());
    set
}
