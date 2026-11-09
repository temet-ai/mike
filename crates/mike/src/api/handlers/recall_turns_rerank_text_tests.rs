use super::{PROMPT_HEAD_CHARS, turn_text};
use crate::analysis::rerank::MAX_DOCUMENT_CHARS;

#[test]
fn turn_text_with_a_long_prompt_and_reply_fits_the_document_cap() {
    let text = turn_text(&"p".repeat(5_000), &"r".repeat(5_000));
    assert!(text.chars().count() <= MAX_DOCUMENT_CHARS);
}

#[test]
fn turn_text_with_a_long_prompt_keeps_the_reply_head() {
    let text = turn_text(&"x".repeat(5_000), "the reply starts here");
    assert_eq!(
        text.chars().filter(|c| *c == 'x').count(),
        PROMPT_HEAD_CHARS
    );
    assert!(text.ends_with("the reply starts here"));
}

#[test]
fn turn_text_with_short_parts_keeps_both_whole() {
    assert_eq!(turn_text("ask", "answer"), "ask\n\nanswer");
}
