use std::collections::BTreeMap;

use super::Candidate;
use crate::llm::decisions::Question;
use crate::llm::decisions::point::noul;

pub const POINT: &str = "lesson_supply";
pub const QUESTION_SET: &str = "lesson_supply_v1";

pub fn key(lesson_id: i64) -> String {
    format!("lesson_{lesson_id}")
}

pub fn questions(lessons: &[Candidate<'_>]) -> BTreeMap<String, Question> {
    lessons
        .iter()
        .map(|lesson| {
            let id = lesson.id;
            (
                key(id),
                noul(
                    &format!(
                        "Given session_context, would the assistant do something different next if it read the lesson at lessons.{id}, compared with not reading it?"
                    ),
                    "The lesson bears on what the assistant is about to do and would change its next step.",
                    "The lesson is about something else, or the assistant would do the same thing without it.",
                ),
            )
        })
        .collect()
}
