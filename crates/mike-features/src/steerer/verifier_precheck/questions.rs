use std::collections::BTreeMap;

use crate::llm::decisions::Question;
use crate::llm::decisions::point::noul;

pub const POINT: &str = "verifier_precheck";
pub const QUESTION_SET: &str = "verifier_precheck_v1";

pub const UNFINISHED: &str = "ask_left_unfinished";
pub const DISCLOSED: &str = "turn_already_says_it";
pub const ONLY_ANSWER: &str = "turn_only_answers";
pub const SENDS_WHEN_TRUE: [&str; 1] = [UNFINISHED];
pub const SENDS_WHEN_FALSE: [&str; 2] = [DISCLOSED, ONLY_ANSWER];

pub fn questions() -> BTreeMap<String, Question> {
    BTreeMap::from([
        (
            UNFINISHED.to_owned(),
            noul(
                "Does the nudge point at something operator_ask asked for that turn_end does not show as done or checked?",
                "The nudge names a real part of the ask that turn_end leaves undone or unchecked.",
                "The nudge names nothing the ask needed, or turn_end shows it done and checked.",
            ),
        ),
        (
            DISCLOSED.to_owned(),
            noul(
                "Does turn_end itself already tell the operator plainly what the nudge asks about: what was not done, not checked, or left for them?",
                "turn_end names the gap in words the operator cannot miss.",
                "turn_end does not name the gap the nudge points at.",
            ),
        ),
        (
            ONLY_ANSWER.to_owned(),
            noul(
                "Is turn_end mainly an answer, an explanation or a report of findings to a question or request for analysis, with no code or configuration changed?",
                "The operator asked for information and turn_end gives it.",
                "turn_end reports work that was built, changed or fixed.",
            ),
        ),
    ])
}
