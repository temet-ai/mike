use std::collections::BTreeMap;

use crate::llm::decisions::Question;
use crate::llm::decisions::point::{choice, noul};

pub const POINT: &str = "ended_on";
pub const QUESTION_SET: &str = "ended_on_v1";
pub const ENDS_ON: &str = "ends_on";
pub const CLAIMS_DONE: &str = "claims_done";

pub const ENDINGS: &[(&str, &str)] = &[
    (
        "report",
        "The turn ends by reporting what was done or found, with nothing left for the operator to answer.",
    ),
    (
        "question",
        "The turn ends by asking the operator a question that needs an answer.",
    ),
    (
        "handback",
        "The turn ends by handing the next step or a decision back to the operator, such as asking whether to go ahead.",
    ),
    (
        "promise",
        "The turn ends by promising or waiting: it will check back, waits on something running, or says what it will do next without doing it.",
    ),
];

pub fn questions() -> BTreeMap<String, Question> {
    BTreeMap::from([
        (
            ENDS_ON.to_owned(),
            choice(
                "What does state.turn_end finish on? Judge the last few sentences.",
                ENDINGS,
            ),
        ),
        (
            CLAIMS_DONE.to_owned(),
            noul(
                "Does state.turn_end claim that the work is done, fixed, finished or verified?",
                "The turn says the work is complete, fixed, finished or verified.",
                "The turn does not claim the work is complete.",
            ),
        ),
    ])
}
