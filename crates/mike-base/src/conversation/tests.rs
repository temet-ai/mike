use std::sync::Arc;

use serde_json::json;

use super::{EarlierState, EarlierTurns, body_messages};

fn turns(raw: &[u8]) -> EarlierTurns {
    let messages = body_messages(raw).unwrap_or_default();
    EarlierTurns::from_chain(Arc::from(messages))
}

#[test]
fn whole_body_has_no_earlier_messages() {
    let earlier = EarlierTurns::whole_body();
    assert_eq!(earlier.state(), EarlierState::WholeBody);
    assert!(earlier.is_empty());
    assert!(!earlier.contains_text("anything"));
}

#[test]
fn last_with_role_finds_newest_assistant() {
    let earlier = turns(
        br#"{"messages":[{"role":"assistant","content":"old"},{"role":"user","content":"q"},{"role":"assistant","content":"new"}]}"#,
    );
    assert_eq!(
        earlier.last_with_role("assistant"),
        Some(json!({"role":"assistant","content":"new"}))
    );
}

#[test]
fn contains_text_finds_marker_in_earlier_message() {
    let earlier = turns(br#"{"messages":[{"role":"user","content":"[lesson:7] be brief"}]}"#);
    assert!(earlier.contains_text("[lesson:7]"));
    assert!(!earlier.contains_text("[lesson:8]"));
    assert!(!earlier.contains_text(""));
}

#[test]
fn conversation_with_appends_body_messages_after_earlier() {
    let earlier = turns(br#"{"messages":[{"role":"user","content":"a"}]}"#);
    let all = earlier.conversation_with(&[json!({"role":"assistant","content":"b"})]);
    assert_eq!(
        all,
        vec![
            json!({"role":"user","content":"a"}),
            json!({"role":"assistant","content":"b"})
        ]
    );
}

#[test]
fn body_messages_without_messages_is_none() {
    assert!(body_messages(br#"{"model":"m"}"#).is_none());
}
