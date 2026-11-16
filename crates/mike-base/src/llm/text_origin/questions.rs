use std::collections::BTreeMap;

use crate::llm::decisions::Question;
use crate::llm::decisions::point::choice;

pub const POINT: &str = "text_origin";
pub const QUESTION_SET: &str = "text_origin_v4";
pub const ORIGIN: &str = "origin";

pub const OPTIONS: &[(&str, &str)] = &[
    (
        "mail",
        "A message delivered from another session or agent through a mailbox or a peer message.",
    ),
    (
        "operator",
        "A person typed this message at the keyboard, even if it is short. A short reply such as 'continue' may repeat an earlier one.",
    ),
    (
        "hook",
        "Output of a hook or a harness notice that was inserted as if the user had said it.",
    ),
    (
        "loop_wakeup",
        "A prompt that tooling re-sends on a timer or schedule to wake the session, such as the prompt of a CronCreate or ScheduleWakeup the session set up. It comes back with the same text each time.",
    ),
    (
        "probe",
        "A probe, sidecar, scheduled job or analysis prompt sent by tooling, such as the brief that starts a non-interactive run, not a task a person typed.",
    ),
];

pub fn questions() -> BTreeMap<String, Question> {
    BTreeMap::from([(
        ORIGIN.to_owned(),
        choice(
            "Who or what wrote `text`? Pick the writer. `follows` is what came before it, and `follows_is` says whether that is the operator's previous message or the end of the assistant's previous reply. `facts.sent_before` says how many earlier user messages in this session have exactly this text, `facts.scheduled_prompt` says whether the session set up a scheduler with this text as its prompt, and `facts.client` says whether a person can type into this session.",
            OPTIONS,
        ),
    )])
}
