use std::collections::BTreeMap;

use crate::llm::decisions::Question;
use crate::llm::decisions::point::noul;

pub const POINT: &str = "commit_message";
pub const QUESTION_SET: &str = "commit_message_v1";

pub const DESCRIBES: &str = "message_describes_diff";
pub const UNMENTIONED: &str = "diff_holds_unmentioned_work";

pub fn questions() -> BTreeMap<String, Question> {
    BTreeMap::from([
        (
            DESCRIBES.to_owned(),
            noul(
                "Does commit_message describe what the changes in diff do?",
                "The message names the real purpose or effect of the changes in the diff.",
                "The message says something other than what the diff changes, or says nothing about it.",
            ),
        ),
        (
            UNMENTIONED.to_owned(),
            noul(
                "Does diff hold a substantial change that commit_message does not mention at all?",
                "The diff contains a separate piece of work, such as another feature, a refactor or a config change, that the message leaves out.",
                "Everything substantial in the diff is covered by the message.",
            ),
        ),
    ])
}
