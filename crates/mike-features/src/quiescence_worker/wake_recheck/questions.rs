use std::collections::BTreeMap;

use crate::llm::decisions::Question;
use crate::llm::decisions::point::noul;

pub const POINT: &str = "wake_recheck";
pub const QUESTION_SET: &str = "wake_recheck_v1";

pub const STILL_OPEN: &str = "concern_still_open";
pub const DONE_SINCE: &str = "done_since_the_note";

pub fn questions() -> BTreeMap<String, Question> {
    BTreeMap::from([
        (
            STILL_OPEN.to_owned(),
            noul(
                "Judging by facts, is what the nudge points at still open now: still unchecked, still unbacked, still not done?",
                "The facts show the concern in the nudge still stands.",
                "The facts show the concern has been dealt with or no longer applies.",
            ),
        ),
        (
            DONE_SINCE.to_owned(),
            noul(
                "Do the facts show the session has done something since the nudge was written that answers it: a check run after the last change, a wait now backed by a job, mail or schedule?",
                "The facts show a check or a backing for the wait that came after the change.",
                "The facts show nothing that answers the nudge.",
            ),
        ),
    ])
}
