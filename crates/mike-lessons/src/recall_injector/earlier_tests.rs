use std::sync::Arc;

use serde_json::json;

use super::*;
use crate::conversation::body_messages;

#[test]
fn collect_assistant_text_continue_body_with_earlier_assistant_reply_matches_full_body()
-> anyhow::Result<()> {
    let earlier_json = json!({"messages": [
        {"role": "user", "content": "hello"},
        {"role": "assistant", "content": [
            {"type": "text", "text": "world <mem:recall topic=\"x\"/>"},
            {"type": "tool_use", "id": "t1", "name": "Bash", "input": {}}
        ]}
    ]});
    let earlier = EarlierTurns::from_chain(Arc::from(
        body_messages(earlier_json.to_string().as_bytes()).unwrap_or_default(),
    ));
    let new_message = json!({"role": "user", "content": [
        {"type": "tool_result", "tool_use_id": "t1", "content": "done"}
    ]});
    let continue_body = json!({
        "thread": {"type": "continue", "previous_message_id": "msg_1"},
        "messages": [new_message.clone()]
    });
    let mut full_messages = earlier.values();
    full_messages.push(new_message);
    let full_body = json!({ "messages": full_messages });

    let text = collect_assistant_text(&continue_body, &earlier);

    assert_eq!(
        text,
        collect_assistant_text(&full_body, &crate::conversation::WHOLE_BODY)
    );
    assert!(text.contains("<mem:recall topic=\"x\"/>"));
    assert_eq!(
        collect_assistant_text(&continue_body, &crate::conversation::WHOLE_BODY),
        ""
    );
    Ok(())
}
