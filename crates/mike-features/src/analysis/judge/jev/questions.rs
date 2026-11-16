use std::collections::BTreeMap;

use serde_json::json;

use crate::llm::decisions::Question;
use crate::llm::decisions::point::{noul, noul_shaped};

pub const POINT: &str = "turn_judge";
pub const QUESTION_SET: &str = "turn_judge_v3";

pub const NAMES_AN_ERROR: &str = "names_an_error";
pub const ASKS_AGAIN: &str = "asks_again";
pub const PUSHES_BACK: &str = "pushes_back";
pub const SOUNDS_FRUSTRATED: &str = "sounds_frustrated";
pub const CORRECTION_SIGNALS: [&str; 4] =
    [NAMES_AN_ERROR, ASKS_AGAIN, PUSHES_BACK, SOUNDS_FRUSTRATED];
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
                "Does `assistant_turn_end` say that an irreversible delivery action happened in this turn: a commit, push, pull request, merge, deploy or release? An offer or a question about doing it (\"Want me to write this up?\") is not shipping.",
                "The text says a commit, push, pull request, merge, deploy or release was made.",
                "The text names no such action as done.",
            ),
        ),
        (
            DELIVERED.to_owned(),
            noul(
                "Does `assistant_turn_end` report that what `operator_request` asked for is done, or answer the question that was asked?",
                "The turn reports the asked-for work finished or the question answered.",
                "The turn does not report the ask as finished or answered.",
            ),
        ),
        (
            LEFT_OPEN.to_owned(),
            noul(
                "Does `assistant_turn_end` say part of the ask is left undone, blocked, deferred or still running?",
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
                "Does `assistant_turn_end` show the result of a check the assistant actually ran, such as a test, a build or a rendered page?",
                "The text shows a check that was run and what it returned.",
                "The text claims an outcome without showing a check, or claims none.",
            ),
        ),
    ])
}

fn correction_questions() -> [(String, Question); 4] {
    [
        (
            NAMES_AN_ERROR.to_owned(),
            noul_shaped(
                json!(
                    "Does `operator_next_message` say that something the assistant did, claimed or produced in `assistant_turn_end` is wrong, broken, misunderstood or missing?"
                ),
                Some(json!({
                    "true": {
                        "what": "The operator points at a fault in the assistant's own work",
                        "examples": ["that's not what I asked", "the build still fails", "you didn't check it"],
                    },
                    "false": {
                        "what": "The operator gives a new instruction, an answer, approval, or reports a problem the assistant did not cause",
                        "examples": ["now do the docs", "yes", "the site is slow today"],
                    },
                })),
            ),
        ),
        (
            ASKS_AGAIN.to_owned(),
            noul_shaped(
                json!({
                    "question": "Does `operator_next_message` ask again for something that `operator_request` already asked for?",
                    "compare": ["operator_request", "operator_next_message"],
                    "note": "A repeat or rephrase of the same ask counts. A follow-up step that builds on the finished work does not.",
                }),
                None,
            ),
        ),
        (
            PUSHES_BACK.to_owned(),
            noul_shaped(
                json!(
                    "Does `operator_next_message` question or reject a choice the assistant made or proposed in `assistant_turn_end`?"
                ),
                Some(json!({
                    "true": {
                        "what": "The operator challenges the assistant's approach",
                        "examples": ["why do we need that, we have uv", "that shouldn't be a thing"],
                    },
                    "false": {
                        "what": "The operator accepts the approach or moves on",
                        "examples": ["ok go ahead", "now deploy it"],
                    },
                })),
            ),
        ),
        (
            SOUNDS_FRUSTRATED.to_owned(),
            noul_shaped(
                json!(
                    "Does `operator_next_message` express frustration or impatience with the assistant?"
                ),
                None,
            ),
        ),
    ]
}

fn reaction_questions() -> BTreeMap<String, Question> {
    let mut asked = BTreeMap::from([
        (
            REDIRECTED.to_owned(),
            noul(
                "Does `operator_next_message` change the direction, scope or priority of the work for reasons that are not the assistant's fault?",
                "The next message moves the work somewhere new without faulting the turn.",
                "The next message keeps to the same line of work.",
            ),
        ),
        (
            ACCEPTED.to_owned(),
            noul(
                "Does `operator_next_message` build on the turn, approve it, or thank for it?",
                "The next message accepts the turn and continues.",
                "The next message does not accept the turn.",
            ),
        ),
    ]);
    asked.extend(correction_questions());
    asked
}

fn own_error_question() -> (String, Question) {
    (
        OWN_ERROR.to_owned(),
        noul(
            "Does the assistant text admit an error or a correction loop, or does `operator_request` reject earlier work (still broken, that is not right, you missed something)?",
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
