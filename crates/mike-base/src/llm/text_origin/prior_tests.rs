use serde_json::json;

use super::prior::{Prior, prior_context};

fn body(messages: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&json!({ "messages": messages })).unwrap_or_default()
}

#[test]
fn prior_context_takes_the_earlier_user_message_and_the_reply_before_the_current_one() {
    let found = prior_context(&body(json!([
        {"role": "user", "content": "build the thing"},
        {"role": "assistant", "content": [{"type": "text", "text": "built it"}]},
        {"role": "user", "content": [{"type": "text", "text": "continue"}]},
    ])));
    assert_eq!(
        found,
        Prior {
            user_message: "build the thing".to_owned(),
            assistant_reply: "built it".to_owned(),
        }
    );
}

#[test]
fn prior_context_skips_tool_results_and_reminder_blocks() {
    let found = prior_context(&body(json!([
        {"role": "user", "content": [
            {"type": "text", "text": "<system-reminder>x</system-reminder>"},
            {"type": "text", "text": "first ask"}
        ]},
        {"role": "assistant", "content": [{"type": "text", "text": "running"}, {"type": "tool_use", "id": "t", "name": "Bash", "input": {}}]},
        {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t", "content": "ok"}]},
        {"role": "assistant", "content": [{"type": "text", "text": "ran it"}]},
        {"role": "user", "content": "second ask"},
    ])));
    assert_eq!(found.user_message, "first ask");
    assert_eq!(found.assistant_reply, "ran it");
}

#[test]
fn prior_context_of_a_first_message_or_bad_body_is_empty() {
    let first = prior_context(&body(json!([{"role": "user", "content": "hi"}])));
    assert_eq!(first, Prior::default());
    assert_eq!(prior_context(b"not json"), Prior::default());
    assert_eq!(prior_context(b"{}"), Prior::default());
}
