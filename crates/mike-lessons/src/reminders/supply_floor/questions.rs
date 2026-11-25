use std::collections::BTreeMap;

use super::Candidate;
use crate::llm::decisions::Question;
use crate::llm::decisions::point::noul;

pub const POINT: &str = "lesson_supply";
pub const QUESTION_SET: &str = "lesson_supply_v3";

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
                        "Does following the lesson at `lessons.{id}` keep the assistant's work in line with the task the operator set in `task`? Where `earlier_ask` and `last_assistant_action` are present, `task` continues them: read it with them."
                    ),
                    "The lesson is about the kind of work `task` asks for, and following it keeps that work on track, e.g. a deploy lesson when the task is to deploy, or a test rule when the task changes the code it covers.",
                    "The lesson is about other work than `task`: another tool, machine, project or job, or work the task does not ask for.",
                ),
            )
        })
        .collect()
}
