use serde_json::json;

use super::{grading_template_from_rows, turn_query, turn_query_with};
use crate::settings::memory_prompts::RERANK_GRADING_PROMPT;
use crate::settings::memory_prompts::RERANK_GRADING_PROMPT_KEY;

#[test]
fn grading_template_without_the_setting_is_the_built_in_text() {
    assert_eq!(grading_template_from_rows(&[]), RERANK_GRADING_PROMPT);
    assert_eq!(
        turn_query("cache", Some("why miss")).as_deref(),
        Some("cache\n\nAsked in the turn: why miss")
    );
}

#[test]
fn grading_template_with_an_edited_setting_fills_query_and_turn() {
    let rows = vec![(
        RERANK_GRADING_PROMPT_KEY.to_owned(),
        json!("Turn: {turn} | Search: {query} {other}"),
    )];
    let template = grading_template_from_rows(&rows);
    assert_eq!(
        turn_query_with(&template, "cache {turn}", Some(" why miss ")).as_deref(),
        Some("Turn: why miss | Search: cache {turn} {other}")
    );
}

#[test]
fn grading_template_with_a_blank_setting_falls_back_to_the_built_in_text() {
    let rows = vec![(RERANK_GRADING_PROMPT_KEY.to_owned(), json!(" \n "))];
    assert_eq!(grading_template_from_rows(&rows), RERANK_GRADING_PROMPT);
}
