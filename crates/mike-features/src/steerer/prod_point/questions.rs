use std::collections::BTreeMap;

use crate::llm::decisions::Question;
use crate::llm::decisions::point::noul;

pub const POINT: &str = "prod_detect";
pub const QUESTION_SET: &str = "prod_detect_v1";
pub const ONLY_NUDGE: &str = "only_nudge";

pub fn questions() -> BTreeMap<String, Question> {
    BTreeMap::from([(
        ONLY_NUDGE.to_owned(),
        noul(
            "Is state.message only a nudge to keep going, with no new content: no instruction, correction, answer, choice or information?",
            "The message only asks the session to continue, report status or wake up, and adds nothing else.",
            "The message adds content: an instruction, a correction, an answer, a choice or information.",
        ),
    )])
}
