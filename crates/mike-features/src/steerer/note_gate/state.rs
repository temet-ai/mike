use serde_json::{Value, json};

use super::NoteKind;

pub const CALLS_KEPT: usize = 40;
pub const ASK_CHARS: usize = 2000;
pub const FINAL_CHARS: usize = 1500;

pub struct NoteInput<'a> {
    pub kind: NoteKind,
    pub note: &'a str,
    pub operator_ask: &'a str,
    pub tool_lines: &'a [String],
    pub final_text: &'a str,
}

pub fn head_chars(text: &str, max: usize) -> &str {
    text.char_indices()
        .nth(max)
        .map_or(text, |(at, _)| &text[..at])
}

pub fn tail_chars(text: &str, max: usize) -> &str {
    let count = text.chars().count();
    if count <= max {
        return text;
    }
    text.char_indices()
        .nth(count - max)
        .map_or(text, |(at, _)| &text[at..])
}

pub fn state(input: &NoteInput<'_>) -> Value {
    let skip = input.tool_lines.len().saturating_sub(CALLS_KEPT);
    let lines: Vec<String> = input
        .tool_lines
        .iter()
        .skip(skip)
        .enumerate()
        .map(|(i, line)| format!("T{i:02}| {line}"))
        .collect();
    json!({
        "note": input.note,
        "operator_ask": head_chars(input.operator_ask, ASK_CHARS),
        "tool_calls": lines,
        "final_text": tail_chars(input.final_text, FINAL_CHARS),
    })
}
