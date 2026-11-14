use std::collections::BTreeMap;

use crate::llm::decisions::Question;
use crate::llm::decisions::point::noul;

pub const POINT: &str = "weasel_after";
pub const QUESTION_SET: &str = "weasel_after_v1";
pub const USED_AS_WEASEL: &str = "used_as_weasel";

pub fn questions() -> BTreeMap<String, Question> {
    BTreeMap::from([(
        USED_AS_WEASEL.to_owned(),
        noul(
            "In state.text, is the word state.word used as a weasel word: vague hedging, empty praise or an excuse where plain facts were needed, rather than in its ordinary meaning?",
            "The word is used to hedge, to praise without evidence, or to excuse something.",
            "The word is used in its ordinary meaning, as part of another word, or as a quoted term.",
        ),
    )])
}
