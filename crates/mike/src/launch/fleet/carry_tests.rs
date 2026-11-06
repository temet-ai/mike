use serde_json::json;

use super::carry::{
    carry_tag, carry_tags, lesson_kind, lesson_text, marked, relevant, slug, warm_block, with_carry,
};

#[test]
fn a_title_becomes_a_lowercase_hyphenated_slug() {
    assert_eq!(
        slug("Run the check once, not 3 times!"),
        "run-the-check-once-not-3-times"
    );
    assert_eq!(
        carry_tag("Ask  the reference seat"),
        "carry:ask-the-reference-seat"
    );
}

#[test]
fn a_title_with_no_letters_or_digits_still_gets_a_tag() {
    assert_eq!(carry_tag("!!!"), "carry:untitled");
}

#[test]
fn only_a_true_carry_flag_marks_a_note() {
    assert!(marked(&json!({"carry": true})));
    assert!(!marked(&json!({"carry": "true"})));
    assert!(!marked(&json!({"carry": false})));
    assert!(!marked(&json!({})));
    assert!(!marked(&json!(null)));
}

#[test]
fn carry_tags_are_the_prefixed_ones_once_each() {
    let tags = vec![
        "finding".to_string(),
        "carry:b".to_string(),
        "carry:a".to_string(),
        "carry:a".to_string(),
        "carry:".to_string(),
    ];
    assert_eq!(
        carry_tags(&tags),
        vec!["carry:a".to_string(), "carry:b".to_string()]
    );
}

#[test]
fn a_note_is_relevant_when_marked_or_tagged() {
    assert!(relevant(&[], &json!({"carry": true})));
    assert!(relevant(&["carry:x".to_string()], &json!({})));
    assert!(!relevant(&["finding".to_string()], &json!({})));
}

#[test]
fn the_lesson_text_is_the_line_when_given_else_the_title_on_one_line() {
    assert_eq!(
        lesson_text("A title", &json!({"line": "  say it\nin one line "})),
        "say it in one line"
    );
    assert_eq!(
        lesson_text("A  title\nbroken", &json!({})),
        "A title broken"
    );
    assert_eq!(lesson_text("A title", &json!({"line": "   "})), "A title");
}

#[test]
fn the_lesson_kind_is_a_known_kind_or_convention() {
    assert_eq!(lesson_kind(&json!({"kind": "warning"})), "warning");
    assert_eq!(lesson_kind(&json!({"kind": "gossip"})), "convention");
    assert_eq!(lesson_kind(&json!({})), "convention");
}

#[test]
fn no_lines_make_no_block_and_leave_the_prompt_alone() {
    assert_eq!(warm_block(&[]), "");
    assert_eq!(warm_block(&["  ".to_string()]), "");
    assert_eq!(with_carry("the prompt", &[]), "the prompt");
}

#[test]
fn each_carried_lesson_is_one_line_after_the_run_part_and_before_the_load_list() {
    let prompt = "You are the template.\n\nThe run's part.\n\nLoad these, in this order, and nothing else:\n1. rules";
    let lines = vec![
        "ask first\nthen search".to_string(),
        "run it once".to_string(),
    ];
    let carried = with_carry(prompt, &lines);
    let run_part = carried.find("The run's part.").unwrap_or(usize::MAX);
    let first = carried
        .find("- ask first then search")
        .unwrap_or(usize::MAX);
    let second = carried.find("- run it once").unwrap_or(usize::MAX);
    let load = carried
        .find("Load these, in this order")
        .unwrap_or(usize::MAX);
    assert!(
        run_part < first && first < second && second < load,
        "{carried}"
    );
    assert!(carried.ends_with("1. rules"));
}

#[test]
fn a_prompt_without_the_load_list_gets_the_block_at_its_end() {
    let carried = with_carry("short", &["one".to_string()]);
    assert!(carried.starts_with("short\n\n"));
    assert!(carried.ends_with("- one"));
}
