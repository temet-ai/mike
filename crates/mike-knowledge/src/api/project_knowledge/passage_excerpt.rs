use std::collections::HashMap;

use serde_json::{Value, json};

use crate::analysis::rerank::MAX_DOCUMENT_CHARS;
use crate::search_ranking::identifier_hits;

pub type Passages = HashMap<String, String>;

pub const EXCERPT_FROM_PASSAGE: &str = "passage";

pub fn head(text: &str, max_chars: usize) -> &str {
    match text.char_indices().nth(max_chars) {
        Some((end, _)) => &text[..end],
        None => text,
    }
}

pub fn show_passage(item: &mut Value, passages: &Passages, max_chars: usize) {
    if identifier_hits(item) > 0 {
        return;
    }
    let passage = item["reference"]
        .as_str()
        .and_then(|reference| passages.get(reference))
        .map(|passage| head(passage, max_chars))
        .filter(|passage| !passage.trim().is_empty());
    let Some(passage) = passage else {
        return;
    };
    let passage = passage.to_owned();
    item["excerpt"] = json!(passage);
    item["excerpt_from"] = json!(EXCERPT_FROM_PASSAGE);
}

pub fn show_reranker_passages(items: &mut [Value], passages: &Passages) {
    for item in items {
        show_passage(item, passages, MAX_DOCUMENT_CHARS);
    }
}

#[cfg(test)]
#[path = "passage_excerpt_tests.rs"]
mod tests;
