use std::collections::BTreeMap;

use crate::llm::decisions::Question;
use crate::llm::decisions::point::choice;

pub const POINT: &str = "text_origin";
pub const QUESTION_SET: &str = "text_origin_v1";
pub const ORIGIN: &str = "origin";

pub const OPTIONS: &[(&str, &str)] = &[
    (
        "operator",
        "A person typed this message at the keyboard, even if it is short or repeats an earlier message.",
    ),
    (
        "mail",
        "A message delivered from another session or agent through a mailbox or a peer message.",
    ),
    (
        "hook",
        "Output of a hook or a harness notice that was inserted as if the user had said it.",
    ),
    (
        "loop_wakeup",
        "A prompt that tooling re-sends on a timer or schedule to wake the session.",
    ),
    (
        "probe",
        "A probe, sidecar or analysis prompt sent by tooling, not a task for the session.",
    ),
];

pub fn questions() -> BTreeMap<String, Question> {
    BTreeMap::from([(
        ORIGIN.to_owned(),
        choice(
            "Who or what wrote state.text? Pick the writer, using state.follows for the message the text came after.",
            OPTIONS,
        ),
    )])
}
