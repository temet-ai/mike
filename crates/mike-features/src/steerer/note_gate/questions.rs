use std::collections::BTreeMap;

use crate::llm::decisions::Question;
use crate::llm::decisions::point::noul;

pub const POINT: &str = "note_gate";
pub const QUESTION_SET: &str = "note_gate_v1";
pub const ABOUT_AT: f64 = 0.7;
pub const OPEN_AT: f64 = 0.75;
pub const ABOUT: &str = "about_this_work";
pub const SAID_OPEN: &str = "said_open";

fn about_this_work() -> Question {
    noul(
        "Is the rule or request quoted in `note` about the kind of work the calls in `tool_calls` do?",
        "The quoted rule or request covers work a call does, e.g. a rule about committing and a call that runs git commit, or a request to fix a page and calls that edit that page.",
        "The quoted rule or request covers work no call does, e.g. a rule about committing when no call commits, setup advice when nothing is being set up, or a request from earlier work that this turn's calls do not touch.",
    )
}

fn said_open() -> Question {
    noul(
        "Does `final_text` say that the thing `note` raises is not done yet or still open?",
        "The final text names that thing as unfinished, pending, not merged or still open.",
        "The final text does not say that thing is unfinished.",
    )
}

pub fn questions() -> BTreeMap<String, Question> {
    BTreeMap::from([
        (ABOUT.to_owned(), about_this_work()),
        (SAID_OPEN.to_owned(), said_open()),
    ])
}
