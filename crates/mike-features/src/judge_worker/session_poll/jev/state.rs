use serde_json::{Value, json};

use crate::analysis::judge::{truncate_chars, truncate_chars_tail};

const FIRST_ASK_CHARS: usize = 800;
const END_CHARS: usize = 6_000;

pub fn state(first_ask: &str, digest: &str, turns: usize) -> Value {
    json!({
        "first_ask": truncate_chars(first_ask, FIRST_ASK_CHARS),
        "session_end": truncate_chars_tail(digest, END_CHARS),
        "operator_turns": turns,
    })
}
