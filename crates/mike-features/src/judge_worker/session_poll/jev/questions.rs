use std::collections::BTreeMap;

use crate::llm::decisions::Question;

pub const POINT: &str = "session_judge";
pub const QUESTION_SET: &str = "session_judge_v2";

pub const OUTCOME: &str = "outcome";
pub const QUALITY: &str = "quality";

pub const QUALITY_LEVELS: [&str; 5] = [
    "The session did not do what the operator asked, or left it broken.",
    "The session did a small part of what was asked.",
    "The session did about half of what was asked.",
    "The session did most of what was asked, with gaps.",
    "The session did everything the operator asked, and showed it working.",
];

pub fn questions() -> BTreeMap<String, Question> {
    BTreeMap::from([
        (
            OUTCOME.to_owned(),
            crate::llm::decisions::point::choice(
                "Judging by how the session ends in session_end, set against the operator's latest asks in recent_asks (first_ask is only where the session began), which outcome describes the session?",
                &[
                    (
                        "success",
                        "What the operator asked for was done by the end of the session.",
                    ),
                    (
                        "partial",
                        "Some of what the operator asked for was done and part was left.",
                    ),
                    (
                        "failure",
                        "The session ended without doing what the operator asked, or ended broken.",
                    ),
                    (
                        "in_progress",
                        "The session is still going or waiting: the end of the text shows work underway, a question put to the operator, or the latest ask not yet answered.",
                    ),
                    (
                        "unknown",
                        "The text does not show how the session went.",
                    ),
                ],
            ),
        ),
        (
            QUALITY.to_owned(),
            Question::Score {
                instructions: "How much of what the operator asked for in recent_asks does session_end show as done and working?".to_owned(),
                criteria: QUALITY_LEVELS.iter().map(|c| (*c).to_owned()).collect(),
            },
        ),
    ])
}
