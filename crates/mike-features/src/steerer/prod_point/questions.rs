use std::collections::BTreeMap;

use crate::llm::decisions::Question;
use crate::llm::decisions::point::noul;

pub const POINT: &str = "prod_detect";
pub const QUESTION_SET: &str = "prod_detect_v2";
pub const ONLY_NUDGE: &str = "only_nudge";

pub fn questions() -> BTreeMap<String, Question> {
    BTreeMap::from([(
        ONLY_NUDGE.to_owned(),
        noul(
            "Is state.message only a nudge to keep going, with no new content? A nudge asks the session to continue, report status or wake up, or is a bare approval or go-ahead with no new instruction (\"yes\", \"do it\", \"go\", \"back to the ask\"). It is not a nudge if it picks between options the assistant offered in state.previous_reply_tail, or adds any instruction, correction or information. state.previous_reply_tail is the end of the assistant reply the message answers, or null.",
            "The message only asks the session to continue, report status or wake up, or is a bare approval or go-ahead, and adds nothing else.",
            "The message picks between options the assistant offered, or adds an instruction, a correction or information.",
        ),
    )])
}
