use serde_json::{Value, json};

use crate::analysis::judge::{truncate_chars, truncate_chars_tail};

const FIRST_ASK_CHARS: usize = 800;
const RECENT_ASK_CHARS: usize = 300;
const RECENT_ASKS: usize = 3;
const END_CHARS: usize = 6_000;

pub fn state(digest: &str, asks: &[&str]) -> Value {
    let recent: Vec<String> = asks
        .iter()
        .skip(asks.len().saturating_sub(RECENT_ASKS))
        .map(|ask| truncate_chars(ask, RECENT_ASK_CHARS).to_owned())
        .collect();
    json!({
        "first_ask": truncate_chars(asks.first().copied().unwrap_or(""), FIRST_ASK_CHARS),
        "recent_asks": recent,
        "session_end": truncate_chars_tail(digest, END_CHARS),
        "operator_turns": asks.len(),
    })
}
