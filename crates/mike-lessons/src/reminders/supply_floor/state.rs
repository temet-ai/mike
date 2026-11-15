use serde_json::{Map, Value, json};

use super::Candidate;
use crate::llm::decisions::text::{head_chars, tail_chars};
use crate::memory_keeper::resolve_text::typed_words;

const CONTEXT_CHARS: usize = 1_500;
const ACTION_CHARS: usize = 300;
const LESSON_CHARS: usize = 700;

pub fn state(context: &str, lessons: &[Candidate<'_>]) -> Value {
    state_with_action(context, None, lessons)
}

pub fn state_with_action(
    context: &str,
    last_assistant_text: Option<&str>,
    lessons: &[Candidate<'_>],
) -> Value {
    let by_id: Map<String, Value> = lessons
        .iter()
        .map(|lesson| {
            (
                lesson.id.to_string(),
                json!({ "kind": lesson.kind, "text": head_chars(lesson.text, LESSON_CHARS) }),
            )
        })
        .collect();
    let mut state = json!({
        "session_context": tail_chars(&typed_words(context), CONTEXT_CHARS),
        "lessons": by_id,
    });
    if let Some(action) = last_assistant_text.map(str::trim).filter(|a| !a.is_empty()) {
        state["last_assistant_action"] = json!(tail_chars(action, ACTION_CHARS));
    }
    state
}
