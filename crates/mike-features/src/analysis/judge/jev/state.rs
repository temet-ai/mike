use serde_json::{Value, json};

use crate::analysis::judge::{truncate_chars, truncate_chars_tail};

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
        "tool_call_count": facts.tool_call_count,
        "tool_error_count": facts.tool_error_count,
    });
    if let (true, Some(next)) = (with_next, facts.next_message) {
        state["operator_next_message"] = Value::String(truncate_chars(next, NEXT_CHARS).to_owned());
    }
    state
}
