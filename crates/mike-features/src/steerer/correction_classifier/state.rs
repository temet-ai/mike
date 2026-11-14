use serde_json::{Value, json};

use crate::llm::decisions::text::{head_chars, tail_chars};

const MESSAGE_CHARS: usize = 2_000;
const PRIOR_END_CHARS: usize = 2_500;

pub fn state(operator_text: &str, prior_turn: &str) -> Value {
    json!({
        "operator_message": head_chars(operator_text, MESSAGE_CHARS),
        "prior_turn_end": tail_chars(prior_turn, PRIOR_END_CHARS),
    })
}
