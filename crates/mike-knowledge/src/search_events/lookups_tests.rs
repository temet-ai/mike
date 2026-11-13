use serde_json::json;

use super::lookups::{MAX_TEXT_REFERENCES, conversation_draft, text_draft};

#[test]
fn conversation_draft_lists_each_session_with_its_match_count_in_order() {
    let sessions = vec![
        ("0b0e4a58-0000-4000-8000-000000000001".to_owned(), 7),
        ("0b0e4a58-0000-4000-8000-000000000002".to_owned(), 1),
    ];

    let draft = conversation_draft("cache rules", &sessions);

    assert_eq!(draft.tool, "search_session_content");
    assert_eq!(draft.query, "cache rules");
    assert_eq!(draft.kind_filter, None);
    assert_eq!(draft.rerank_state, "off");
    assert_eq!(
        draft.returned,
        json!([
            {"reference": "session:0b0e4a58-0000-4000-8000-000000000001", "matches": 7},
            {"reference": "session:0b0e4a58-0000-4000-8000-000000000002", "matches": 1}
        ])
    );
}

#[test]
fn conversation_draft_with_no_sessions_has_an_empty_return() {
    assert_eq!(conversation_draft("nothing", &[]).returned, json!([]));
}

#[test]
fn text_draft_lists_each_request_once_in_the_order_it_first_matched() {
    let draft = text_draft("needle-x", [30, 10, 30, 20, 10]);

    assert_eq!(draft.tool, "find_text");
    assert_eq!(draft.query, "needle-x");
    assert_eq!(draft.rerank_state, "off");
    assert_eq!(
        draft.returned,
        json!([
            {"reference": "request:30"},
            {"reference": "request:10"},
            {"reference": "request:20"}
        ])
    );
}

#[test]
fn text_draft_keeps_at_most_fifty_requests() {
    let draft = text_draft("needle-x", 1..=80);

    let returned = draft.returned.as_array().map(Vec::as_slice).unwrap_or(&[]);
    assert_eq!(returned.len(), MAX_TEXT_REFERENCES);
    assert_eq!(returned[49], json!({"reference": "request:50"}));
}
