use super::*;

#[test]
fn assistant_text_is_kept_from_the_end_where_the_delivery_is() {
    let turn = "Let me start by reading the tests. \
                I'll check the config next. \
                Done: the clearance is now 0.4mm and the gate passes.";
    let kept = truncate_chars_tail(turn, 60);
    assert!(
        kept.contains("the clearance is now 0.4mm"),
        "the delivery must survive truncation, got: {kept:?}"
    );
    assert!(
        !kept.contains("Let me start by"),
        "the opening narration is what should be dropped, got: {kept:?}"
    );
    assert_eq!(kept.chars().count(), 60);
}

#[test]
fn a_short_turn_is_not_truncated_at_all() {
    assert_eq!(truncate_chars_tail("all done", 3_000), "all done");
    assert_eq!(truncate_chars_tail("", 3_000), "");
}

#[test]
fn head_and_tail_truncation_take_opposite_ends() {
    let s = "FIRST middle LAST";
    assert!(truncate_chars(s, 5).contains("FIRST"));
    assert!(truncate_chars_tail(s, 4).contains("LAST"));
}

#[test]
fn truncate_tail_never_panics_on_multibyte_boundary() {
    let s = "→".repeat(50);
    for n in 0..60 {
        let out = truncate_chars_tail(&s, n);
        assert!(out.chars().count() <= 50);
    }
}

#[test]
fn truncate_chars_never_panics_on_multibyte_boundary() {
    let s = "→".repeat(2_000);
    let out = truncate_chars(&s, 1_000);
    assert_eq!(out.chars().count(), 1_000);
    assert_eq!(out, "→".repeat(1_000));
}

#[test]
fn truncate_chars_returns_whole_string_when_under_cap() {
    let s = "short ünïcode";
    assert_eq!(truncate_chars(s, 1_000), s);
}

#[test]
fn user_message_contains_all_sections() {
    let c = JudgeCandidateRow {
        turn_id: 42,
        user_prompt: "Fix the bug in lib.rs".to_owned(),
        final_assistant_text: "I have fixed the bug.".to_owned(),
        tool_call_count: 3,
        tool_error_count: 1,
    };
    let msg = build_judge_user_message(&c);
    assert!(msg.contains("OPERATOR REQUEST"));
    assert!(msg.contains("ASSISTANT TURN TEXT"));
    assert!(msg.contains("TOOL USE SUMMARY"));
    assert!(msg.contains("Fix the bug in lib.rs"));
    assert!(msg.contains("I have fixed the bug."));
    assert!(msg.contains("tool_call_count = 3"));
    assert!(msg.contains("tool_error_count = 1"));
}

#[test]
fn user_message_truncates_long_assistant_text() {
    let long_text = "x".repeat(4_000);
    let c = JudgeCandidateRow {
        turn_id: 1,
        user_prompt: "Do something".to_owned(),
        final_assistant_text: long_text.clone(),
        tool_call_count: 0,
        tool_error_count: 0,
    };
    let msg = build_judge_user_message(&c);
    assert!(msg.len() < long_text.len() + 1_000 + 200);
}

#[test]
fn user_message_keeps_the_turn_ending_not_its_opening() {
    let c = JudgeCandidateRow {
        turn_id: 7,
        user_prompt: "Fix the failing gate".to_owned(),
        final_assistant_text: format!(
            "OPENING-NARRATION {} CLOSING-DELIVERY: committed and pushed.",
            "filler ".repeat(1_000)
        ),
        tool_call_count: 12,
        tool_error_count: 0,
    };
    let msg = build_judge_user_message(&c);
    assert!(
        msg.contains("CLOSING-DELIVERY: committed and pushed."),
        "the turn's delivery must survive truncation"
    );
    assert!(
        !msg.contains("OPENING-NARRATION"),
        "the opening narration is what should be cut, not the delivery"
    );
}
