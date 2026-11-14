use serde_json::{Map, Value, json};

use super::questions::key;
use crate::llm::decisions::text::head_chars;
use crate::store::models::Diagnostic;

const SENTENCE_CHARS: usize = 400;

pub fn sentence_around(text: &str, range: (usize, usize)) -> &str {
    let ends = |c: char| matches!(c, '.' | '!' | '?' | '\n');
    if range.0 >= text.len() {
        return text;
    }
    let from = range.0;
    let to = range.1.min(text.len()).max(from);
    if !text.is_char_boundary(from) || !text.is_char_boundary(to) {
        return text;
    }
    let start = text[..from].rfind(ends).map_or(0, |i| {
        i + text[i..].chars().next().map_or(1, char::len_utf8)
    });
    let end = text[to..].find(ends).map_or(text.len(), |i| {
        to + i + text[to + i..].chars().next().map_or(1, char::len_utf8)
    });
    text[start..end].trim()
}

pub fn state(text: &str, diagnostics: &[Diagnostic]) -> Value {
    let matches: Map<String, Value> = diagnostics
        .iter()
        .enumerate()
        .map(|(i, d)| {
            (
                key(i),
                json!({
                    "phrase": d.matched_text,
                    "category": d.category.as_str(),
                    "sentence": head_chars(sentence_around(text, d.match_range), SENTENCE_CHARS),
                }),
            )
        })
        .collect();
    json!({ "matches": matches })
}
