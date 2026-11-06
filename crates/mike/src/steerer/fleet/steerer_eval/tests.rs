mod abstention_test;
mod command_failed_test;
mod delivery_test;
mod effective_client_test;
mod gate_wording_test;
mod input_budget_test;
mod mailbox_pending_test;
mod note_limit_test;
mod quorum_and_evidence_tests;
mod rollout_kind_test;
mod stays_quiet_test;
mod support;
mod suppressed_early_test;

use super::*;

#[test]
fn pattern_matches_case_insensitive() {
    assert!(pattern_matches("SHIP IT\ndone", "I'll ship it now"));
    assert!(pattern_matches("done\ncomplete", "I'm DONE with this task"));
    assert!(!pattern_matches("refocus", "just a normal message"));
    assert!(!pattern_matches("", "some text"));
    assert!(!pattern_matches("keyword", ""));
    assert!(!pattern_matches("shipit", "ship it"));
}

#[test]
fn pattern_matches_multiline_first_hit() {
    assert!(pattern_matches("alpha\nbeta\ngamma", "found beta here"));
    assert!(!pattern_matches("alpha\nbeta\ngamma", "no match at all"));
}

#[test]
fn pattern_invocation_matches_no_cat_in_bash_seed_pattern() {
    const NO_CAT_IN_BASH_PATTERN: &str = "cat \ncat<<\n| cat";

    let anti_pattern_turn = EvalInputs {
        eval_text: "Sure, let me check that file for you.".to_string(),
        operator_ask: "can you check that config?".to_string(),
        last_user_message: "can you check that config?".to_string(),
        tool_calls_summary: "1. Bash — cat tool-results/spill.txt\n".to_string(),
        diagnostics: Vec::new(),
        anchor: None,
        trigger_point: "response",
        turn_origin: String::new(),
        subagent_persona: None,
        judged_turn_id: None,
        evaluation_turn_id: None,
        question: None,
    };
    assert!(pattern_invocation_matches(
        NO_CAT_IN_BASH_PATTERN,
        &anti_pattern_turn
    ));

    let clean_turn = EvalInputs {
        eval_text: "Sure, let me check that file for you.".to_string(),
        operator_ask: "can you check that config?".to_string(),
        last_user_message: "can you check that config?".to_string(),
        tool_calls_summary: "1. Read — README.md\n".to_string(),
        diagnostics: Vec::new(),
        anchor: None,
        trigger_point: "response",
        turn_origin: String::new(),
        subagent_persona: None,
        judged_turn_id: None,
        evaluation_turn_id: None,
        question: None,
    };
    assert!(!pattern_invocation_matches(
        NO_CAT_IN_BASH_PATTERN,
        &clean_turn
    ));
}

#[test]
fn pattern_invocation_matches_eval_text_when_tool_summary_empty() {
    let inputs = EvalInputs {
        eval_text: "The task is done.".to_string(),
        operator_ask: "please finish".to_string(),
        last_user_message: "please finish".to_string(),
        tool_calls_summary: String::new(),
        diagnostics: Vec::new(),
        anchor: None,
        trigger_point: "response",
        turn_origin: String::new(),
        subagent_persona: None,
        judged_turn_id: None,
        evaluation_turn_id: None,
        question: None,
    };
    assert!(pattern_invocation_matches("done", &inputs));
    assert!(!pattern_invocation_matches("cat ", &inputs));
}
