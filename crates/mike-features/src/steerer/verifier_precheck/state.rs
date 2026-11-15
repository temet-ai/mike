use serde_json::{Value, json};

use super::facts::Facts;
use crate::llm::decisions::text::{head_chars, tail_chars};
use crate::steerer::verdict::SteererVerdict;

const ASK_CHARS: usize = 800;
const END_CHARS: usize = 2_500;
const NUDGE_CHARS: usize = 600;

pub struct FireInput<'a> {
    pub operator_ask: &'a str,
    pub turn_end: &'a str,
    pub nudge: &'a str,
    pub verdict: &'a SteererVerdict,
}

pub fn state(input: &FireInput<'_>) -> Value {
    json!({
        "ask": head_chars(input.operator_ask, ASK_CHARS),
        "turn_end": tail_chars(input.turn_end, END_CHARS),
        "nudge": head_chars(input.nudge, NUDGE_CHARS),
    })
}

pub fn state_with(input: &FireInput<'_>, facts: &Facts) -> Value {
    let mut state = state(input);
    if input.operator_ask.trim().is_empty() {
        state["ask"] = json!(head_chars(&facts.judged_ask, ASK_CHARS));
    }
    if let (Some(into), Value::Object(from)) = (state.as_object_mut(), facts.json()) {
        into.extend(from);
    }
    state
}
