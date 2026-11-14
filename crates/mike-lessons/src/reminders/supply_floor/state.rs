use serde_json::{Map, Value, json};

use super::Candidate;
use crate::llm::decisions::text::{head_chars, tail_chars};

const CONTEXT_CHARS: usize = 1_500;
const LESSON_CHARS: usize = 700;

pub fn state(context: &str, lessons: &[Candidate<'_>]) -> Value {
    let by_id: Map<String, Value> = lessons
        .iter()
        .map(|lesson| {
            (
                lesson.id.to_string(),
                json!({ "kind": lesson.kind, "text": head_chars(lesson.text, LESSON_CHARS) }),
            )
        })
        .collect();
    json!({
        "session_context": tail_chars(context, CONTEXT_CHARS),
        "lessons": by_id,
    })
}
