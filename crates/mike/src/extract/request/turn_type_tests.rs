use super::*;
use serde_json::json;

#[test]
fn turn_type_user_prompt_for_normal_conversation() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "Hello"},
            {"role": "assistant", "content": "Hi"},
            {"role": "user", "content": "How are you?"}
        ]
    });
    assert_eq!(extract_turn_type(&body), TurnType::UserPrompt);
    assert_eq!(extract_turn_type(&body).to_string(), "user_prompt");
}

#[test]
fn turn_type_tool_loop_when_last_msg_is_tool_result() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "Do something"},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "bash", "input": {"command": "ls"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "file.txt"}
            ]}
        ]
    });
    assert_eq!(extract_turn_type(&body), TurnType::ToolLoop);
    assert_eq!(extract_turn_type(&body).to_string(), "tool_loop");
}

#[test]
fn turn_type_continuation_when_no_user_messages() {
    let body = json!({
        "messages": [
            {"role": "assistant", "content": "I was thinking..."}
        ]
    });
    assert_eq!(extract_turn_type(&body), TurnType::Continuation);
    assert_eq!(extract_turn_type(&body).to_string(), "continuation");
}

#[test]
fn turn_type_tool_loop_for_single_user_msg_with_json_object_content() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "{\"session\":{\"limit\":10,\"model_breakdown\":[]}}"}
        ]
    });
    assert_eq!(extract_turn_type(&body), TurnType::ToolLoop);
}

#[test]
fn turn_type_tool_loop_for_single_user_msg_with_json_array_content() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "[{\"id\":1},{\"id\":2}]"}
        ]
    });
    assert_eq!(extract_turn_type(&body), TurnType::ToolLoop);
}

#[test]
fn turn_type_user_prompt_for_single_user_msg_with_plain_text_content() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "can you help me with this?"}
        ]
    });
    assert_eq!(extract_turn_type(&body), TurnType::UserPrompt);
}

#[test]
fn turn_type_user_prompt_for_single_user_msg_with_whitespace_and_no_json() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "  looks like a config { not really json }"}
        ]
    });
    assert_eq!(extract_turn_type(&body), TurnType::UserPrompt);
}
