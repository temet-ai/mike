use serde_json::{Value, json};

use crate::analysis::judge::{truncate_chars, truncate_chars_tail};
use crate::llm::decisions::operator_text::typed_by_operator;

const REQUEST_CHARS: usize = 1_000;
const TURN_END_CHARS: usize = 3_000;
const NEXT_CHARS: usize = 2_000;

#[derive(Debug, Clone, Copy)]
pub struct TurnFacts<'a> {
    pub turn_id: i64,
    pub session_id: Option<i64>,
    pub operator_request: &'a str,
    pub assistant_text: &'a str,
    pub tool_call_count: i32,
    pub tool_error_count: i32,
    pub next_message: Option<&'a str>,
}

pub fn state(facts: &TurnFacts<'_>, with_next: bool) -> Value {
    let mut state = json!({
        "operator_request": truncate_chars(facts.operator_request, REQUEST_CHARS),
        "assistant_turn_end": truncate_chars_tail(facts.assistant_text, TURN_END_CHARS),
    });
    if let (true, Some(next)) = (with_next, present(facts.next_message)) {
        state["next_message_repeats_the_request"] =
            Value::Bool(repeats(facts.operator_request, &next));
        state["operator_next_message"] =
            Value::String(truncate_chars(&next, NEXT_CHARS).to_owned());
    }
    state
}

fn repeats(request: &str, next: &str) -> bool {
    let request = typed_by_operator(request).unwrap_or_else(|| request.trim().to_owned());
    next.trim() == request
}

pub fn present(text: Option<&str>) -> Option<String> {
    text.and_then(typed_by_operator)
}
