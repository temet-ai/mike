use serde_json::json;

use super::*;

fn call_message(name: &str, id: &str, query: &str) -> Value {
    json!({"role": "assistant", "content": [
        {"type": "tool_use", "id": id, "name": name, "input": {"needle": query}}
    ]})
}

fn result_message(id: &str, text: &str) -> Value {
    json!({"role": "user", "content": [
        {"type": "tool_result", "tool_use_id": id, "content": text}
    ]})
}

#[test]
fn is_search_tool_name_prefixed_and_bare_names_match() {
    assert!(is_search_tool_name("mcp__mike__find_text"));
    assert!(is_search_tool_name("find_text"));
    assert!(is_search_tool_name("mcp__mike__recall_project_context"));
    assert!(!is_search_tool_name("Bash"));
    assert!(!is_search_tool_name("find_text_helper"));
}

#[test]
fn claims_message_search_call_with_needle_is_claimed_and_its_result_follows() {
    let mut own = OwnSearchCalls::default();
    let call = call_message("mcp__mike__find_text", "toolu_1", "needle-x");
    assert!(own.claims_message(&call, "needle-x", true));
    let result = result_message("toolu_1", "echo needle-x");
    assert!(own.claims_message(&result, "needle-x", true));
}

#[test]
fn claims_message_search_call_without_needle_is_not_claimed() {
    let mut own = OwnSearchCalls::default();
    let call = call_message("mcp__mike__find_text", "toolu_2", "something else");
    assert!(!own.claims_message(&call, "needle-x", true));
    assert!(!own.claims_message(&result_message("toolu_2", "needle-x"), "needle-x", true));
}

#[test]
fn claims_message_other_tool_with_needle_is_not_claimed() {
    let mut own = OwnSearchCalls::default();
    let call = call_message("Bash", "toolu_3", "needle-x");
    assert!(!own.claims_message(&call, "needle-x", true));
}

#[test]
fn claims_message_result_of_unknown_call_is_not_claimed() {
    let mut own = OwnSearchCalls::default();
    assert!(!own.claims_message(&result_message("toolu_9", "needle-x"), "needle-x", true));
}

#[test]
fn claims_message_case_insensitive_needle_matches_input() {
    let mut own = OwnSearchCalls::default();
    let call = call_message("mcp__mike__search_session_content", "toolu_4", "NEEDLE-X");
    assert!(own.claims_message(&call, "needle-x", false));
    assert!(!OwnSearchCalls::default().claims_message(&call, "needle-x", true));
}

#[test]
fn claims_message_string_content_is_not_claimed() {
    let mut own = OwnSearchCalls::default();
    let message = json!({"role": "user", "content": "needle-x"});
    assert!(!own.claims_message(&message, "needle-x", true));
}
