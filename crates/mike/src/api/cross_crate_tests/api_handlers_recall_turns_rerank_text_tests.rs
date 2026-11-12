//! Moved from crates/mike/src/api/handlers/recall_turns_rerank_text_tests.rs by the crate split: it reaches the web
//! crate, so it is tested from there.

use crate::analysis::rerank::MAX_DOCUMENT_CHARS;
use crate::api::handlers::recall_turns_rerank::{PROMPT_HEAD_CHARS, turn_text};

const HEADER: &str = "turn | 2026-11-08 | Search work";

#[test]
fn turn_text_with_a_long_prompt_and_reply_fits_the_document_cap() {
    let text = turn_text(HEADER, &"p".repeat(5_000), &"r".repeat(5_000));
    assert!(text.chars().count() <= MAX_DOCUMENT_CHARS);
}

#[test]
fn turn_text_with_a_long_prompt_keeps_the_reply_head() {
    let text = turn_text(HEADER, &"x".repeat(5_000), "the reply starts here");
    assert_eq!(
        text.chars().filter(|c| *c == 'x').count(),
        PROMPT_HEAD_CHARS
    );
    assert!(text.ends_with("the reply starts here"));
}

#[test]
fn turn_text_with_short_parts_keeps_both_whole_under_the_header() {
    assert_eq!(
        turn_text(HEADER, "ask", "answer"),
        "turn | 2026-11-08 | Search work\nask\n\nanswer"
    );
}
