use std::collections::BTreeMap;

use crate::llm::decisions::Question;
use crate::llm::decisions::point::noul;

pub const POINT: &str = "verifier_precheck";
pub const QUESTION_SET: &str = "verifier_precheck_v2";

pub const UNFINISHED: &str = "ask_left_unfinished";
pub const DISCLOSED: &str = "turn_already_says_it";
pub const ONLY_ANSWER: &str = "turn_only_answers";
pub const STILL_RUNNING: &str = "work_still_running";
pub const NOT_OPERATOR_ASK: &str = "ask_not_in_operator_words";
pub const DONE_EARLIER: &str = "done_earlier";

pub const VETOES: [&str; 3] = [STILL_RUNNING, NOT_OPERATOR_ASK, DONE_EARLIER];

pub fn questions() -> BTreeMap<String, Question> {
    BTreeMap::from([
        (
            STILL_RUNNING.to_owned(),
            noul(
                "Does turn_end say the work is still under way rather than finished: a build, push, gate, agent, check or deploy is still running, or it names the next step it is about to take?",
                "turn_end reports work in progress or a next step it is about to take, not a finished result.",
                "turn_end presents the work as finished.",
            ),
        ),
        (
            NOT_OPERATOR_ASK.to_owned(),
            noul(
                "Is the thing the nudge says was asked for missing from the operator's own words in operator_ask, so that it comes from machine text such as acceptance criteria, a checklist, a heading, hook output or a template?",
                "The operator never asked for what the nudge names; it comes from generated or machine text.",
                "operator_ask, in the operator's own words, asks for what the nudge names.",
            ),
        ),
        (
            DONE_EARLIER.to_owned(),
            noul(
                "Does turn_end say that what the nudge asks about was already done or answered before this turn, naming a commit, a time, an earlier reply or something the operator already has?",
                "turn_end says it was done or answered earlier.",
                "turn_end does not say it was done or answered earlier.",
            ),
        ),
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
