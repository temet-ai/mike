use serde_json::{Map, Value, json};

use super::Candidate;
use crate::lesson_delivery::prompt_context::PromptContext;
use crate::llm::decisions::text::{head_chars, tail_chars};
use crate::memory_keeper::resolve_text::typed_words;

const CONTEXT_CHARS: usize = 1_500;
const LESSON_CHARS: usize = 700;

pub fn state(context: &str, earlier: &PromptContext, lessons: &[Candidate<'_>]) -> Value {
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
        "task": tail_chars(&typed_words(context), CONTEXT_CHARS),
        "lessons": by_id,
    });
    if !earlier.earlier_ask.is_empty() {
        state["earlier_ask"] = json!(earlier.earlier_ask);
    }
    if !earlier.last_action.is_empty() {
        state["last_assistant_action"] = json!(earlier.last_action);
    }
    state
}
