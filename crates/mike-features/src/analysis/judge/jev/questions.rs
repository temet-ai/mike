use std::collections::BTreeMap;

use crate::llm::decisions::Question;
use crate::llm::decisions::point::noul;

pub const POINT: &str = "turn_judge";
pub const QUESTION_SET: &str = "turn_judge_v1";

pub const CORRECTED: &str = "operator_corrected";
pub const REDIRECTED: &str = "operator_redirected";
pub const ACCEPTED: &str = "operator_accepted";
pub const OWN_ERROR: &str = "turn_own_error";
pub const ABANDONED: &str = "no_real_work";
pub const SHIPPED: &str = "shipped_irreversibly";
pub const DELIVERED: &str = "delivered_the_ask";
pub const LEFT_OPEN: &str = "part_left_open";
pub const ASKS_OPERATOR: &str = "asks_the_operator";
pub const CHECKED: &str = "backed_by_a_check";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Legacy,
    TwoAxis { has_next: bool },
}

fn turn_questions() -> BTreeMap<String, Question> {
    BTreeMap::from([
        (
            ABANDONED.to_owned(),
            noul(
                "Did the assistant turn produce no real work: it refused, only made a plan, or stopped before doing anything?",
                "The turn ends with a refusal, a bare plan or an interruption and no work done.",
                "The turn did real work toward the operator request.",
            ),
        ),
        (
            SHIPPED.to_owned(),
            noul(
                "Does assistant_turn_end say that an irreversible delivery action happened in this turn: a commit, push, pull request, merge, deploy or release?",
                "The text says a commit, push, pull request, merge, deploy or release was made.",
                "The text names no such action as done.",
            ),
        ),
        (
            DELIVERED.to_owned(),
            noul(
                "Does assistant_turn_end report that what operator_request asked for is done, or answer the question that was asked?",
                "The turn reports the asked-for work finished or the question answered.",
                "The turn does not report the ask as finished or answered.",
            ),
        ),
        (
            LEFT_OPEN.to_owned(),
            noul(
                "Does assistant_turn_end say part of the ask is left undone, blocked, deferred or still running?",
                "The text names a part of the ask that is not finished.",
                "The text names no unfinished part.",
            ),
        ),
        (
            ASKS_OPERATOR.to_owned(),
            noul(
                "Is the main output of the turn a question or a decision put to the operator, with little of the ask done?",
                "The turn mostly asks the operator something.",
                "The turn mostly reports work or an answer.",
            ),
        ),
        (
            CHECKED.to_owned(),
            noul(
                "Does assistant_turn_end show the result of a check the assistant actually ran, such as a test, a build or a rendered page?",
                "The text shows a check that was run and what it returned.",
                "The text claims an outcome without showing a check, or claims none.",
            ),
        ),
    ])
}

fn reaction_questions() -> BTreeMap<String, Question> {
    BTreeMap::from([
        (
            CORRECTED.to_owned(),
            noul(
                "Does operator_next_message say or imply the assistant got something wrong in the turn: a mistake, a misread ask, an unchecked claim, an ignored instruction, or work not done? Re-asking for the same thing counts.",
                "The next message corrects, challenges or re-asks for something the turn got wrong.",
                "The next message does not fault the turn.",
            ),
        ),
        (
            REDIRECTED.to_owned(),
            noul(
                "Does operator_next_message change the direction, scope or priority of the work for reasons that are not the assistant's fault?",
                "The next message moves the work somewhere new without faulting the turn.",
                "The next message keeps to the same line of work.",
            ),
        ),
        (
            ACCEPTED.to_owned(),
            noul(
                "Does operator_next_message build on the turn, approve it, thank for it, or give the next step in the same line of work?",
                "The next message accepts the turn and continues.",
                "The next message does not accept the turn.",
            ),
        ),
    ])
}

fn own_error_question() -> (String, Question) {
    (
        OWN_ERROR.to_owned(),
        noul(
            "Does the assistant text admit an error or a correction loop, or does operator_request reject earlier work (still broken, that is not right, you missed something)?",
            "The turn admits a mistake or the request rejects earlier work.",
            "Neither the text nor the request points at a mistake.",
        ),
    )
}

pub fn questions(shape: Shape) -> BTreeMap<String, Question> {
    let mut asked = turn_questions();
    match shape {
        Shape::Legacy => {
            let (key, question) = own_error_question();
            asked.insert(key, question);
        }
        Shape::TwoAxis { has_next: true } => asked.extend(reaction_questions()),
        Shape::TwoAxis { has_next: false } => {}
    }
    asked
}
