use std::collections::BTreeMap;

use crate::llm::decisions::Question;
use crate::llm::decisions::point::noul;

pub const POINT: &str = "correction_classifier";
pub const QUESTION_SET: &str = "correction_classifier_v1";
pub const CORRECTS: &str = "corrects_prior_turn";

pub fn questions() -> BTreeMap<String, Question> {
    BTreeMap::from([(
        CORRECTS.to_owned(),
        noul(
            "Does operator_message say or imply that the assistant got something wrong in prior_turn_end: a mistake, a misread ask, a false claim, an ignored instruction, or work that was not done? Asking for the same thing again counts.",
            "The message corrects, challenges or rejects what the assistant said or did in the prior turn.",
            "The message adds to the work, answers a question, approves, or changes the subject without faulting the prior turn.",
        ),
    )])
}
