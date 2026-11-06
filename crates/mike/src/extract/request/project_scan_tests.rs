use super::*;
use serde_json::json;

#[test]
fn collect_sources_from_string_content() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "hello"},
            {"role": "assistant", "content": "hi"}
        ]
    });
    let sources = collect_project_scan_sources(&body);
    assert_eq!(sources, vec!["hello", "hi"]);
}

#[test]
fn collect_sources_from_text_blocks() {
    let body = json!({
        "messages": [
            {"role": "user", "content": [
                {"type": "text", "text": "a"},
                {"type": "text", "text": "b"}
            ]}
        ]
    });
    let sources = collect_project_scan_sources(&body);
    assert_eq!(sources, vec!["a", "b"]);
}

#[test]
fn collect_sources_recurses_into_tool_result_string_content() {
    let body = json!({
        "messages": [
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "nested-text"}
            ]}
        ]
    });
    let sources = collect_project_scan_sources(&body);
    assert_eq!(sources, vec!["nested-text"]);
}

#[test]
fn collect_sources_recurses_into_tool_result_array_content() {
    let body = json!({
        "messages": [
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": [
                    {"type": "text", "text": "part1"},
                    {"type": "text", "text": "part2"}
                ]}
            ]}
        ]
    });
    let sources = collect_project_scan_sources(&body);
    assert_eq!(sources, vec!["part1", "part2"]);
}

#[test]
fn collect_sources_empty_when_no_messages() {
    let body = json!({});
    let sources = collect_project_scan_sources(&body);
    assert!(sources.is_empty());
}
