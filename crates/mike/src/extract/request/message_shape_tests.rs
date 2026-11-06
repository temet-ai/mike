use super::*;
use serde_json::json;

use super::super::extract_message_count;

#[test]
fn tool_loop_detected() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "user", "content": "Do something"},
            {"role": "assistant", "content": [
                {"type": "text", "text": "I'll use a tool"},
                {"type": "tool_use", "id": "t1", "name": "bash", "input": {"command": "ls"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "file.txt"}
            ]}
        ]
    });
    assert!(detect_tool_loop(&body));
    Ok(())
}

#[test]
fn tool_loop_not_detected_normal_conversation() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "user", "content": "Hello"},
            {"role": "assistant", "content": "Hi"},
            {"role": "user", "content": "How are you?"}
        ]
    });
    assert!(!detect_tool_loop(&body));
    Ok(())
}

#[test]
fn tool_loop_too_few_messages() -> anyhow::Result<()> {
    let body = json!({"messages": [{"role": "user", "content": "Hi"}]});
    assert!(!detect_tool_loop(&body));
    Ok(())
}

#[test]
fn tool_loop_empty_messages() -> anyhow::Result<()> {
    let body = json!({"messages": []});
    assert!(!detect_tool_loop(&body));
    Ok(())
}

#[test]
fn tool_loop_missing_messages() -> anyhow::Result<()> {
    let body = json!({});
    assert!(!detect_tool_loop(&body));
    Ok(())
}

#[test]
fn tool_loop_detected_when_user_content_is_raw_json_object_string() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "user", "content": "list sessions"},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "list_sessions", "input": {}}
            ]},
            {"role": "user", "content": "{\"session\":{\"limit\":10,\"model_breakdown\":[]}}"}
        ]
    });
    assert!(detect_tool_loop(&body));
    Ok(())
}

#[test]
fn tool_loop_detected_when_user_content_is_raw_json_array_string() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "list_requests", "input": {}}
            ]},
            {"role": "user", "content": "[{\"id\":1},{\"id\":2}]"}
        ]
    });
    assert!(detect_tool_loop(&body));
    Ok(())
}

#[test]
fn tool_loop_not_detected_when_user_content_is_plain_text_after_tool_use() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "bash", "input": {}}
            ]},
            {"role": "user", "content": "ok that looks fine"}
        ]
    });
    assert!(!detect_tool_loop(&body));
    Ok(())
}

#[test]
fn tool_loop_no_content_array_in_assistant() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "assistant", "content": "just text"},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "result"}
            ]}
        ]
    });
    assert!(!detect_tool_loop(&body));
    Ok(())
}

#[test]
fn tool_loop_detected_for_classic_two_message_assistant_tool_use_body() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "bash", "input": {"command": "ls"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "file.txt"}
            ]}
        ]
    });
    assert!(detect_tool_loop(&body));
    Ok(())
}

#[test]
fn tool_loop_detected_for_thread_mode_single_tool_result_message() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "user", "content": [
                {"tool_use_id": "toolu_011Cf9QtvURXXrcgJrnJ2KKT", "type": "tool_result", "content": "The file has been updated successfully."}
            ]},
            {"role": "system", "content": [
                {"type": "text", "text": "<system-reminder>\n<total_tokens>1000</total_tokens>\n</system-reminder>", "cache_control": {"type": "ephemeral"}}
            ]}
        ]
    });
    assert!(detect_tool_loop(&body));
    Ok(())
}

#[test]
fn tool_loop_not_detected_for_operator_prompt_with_trailing_system_message() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "user", "content": "Refactor the session store to use the new schema."},
            {"role": "system", "content": [
                {"type": "text", "text": "<system-reminder>\n<total_tokens>1000</total_tokens>\n</system-reminder>"}
            ]}
        ]
    });
    assert!(!detect_tool_loop(&body));
    Ok(())
}

#[test]
fn tool_loop_detected_with_trailing_system_message() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "user", "content": "Do something"},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "bash", "input": {"command": "ls"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "file.txt"}
            ]},
            {"role": "system", "content": "mid-session policy update"}
        ]
    });
    assert!(detect_tool_loop(&body));
    Ok(())
}

#[test]
fn fingerprint_count_ignores_trailing_system_but_real_count_does_not() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "user", "content": "hi"},
            {"role": "system", "content": "policy"}
        ]
    });
    assert_eq!(
        extract_message_count(&body),
        2,
        "honest count includes system"
    );
    assert_eq!(
        fingerprint_message_count(&body),
        1,
        "fingerprint count drops the trailing system entry"
    );
    Ok(())
}

#[test]
fn fingerprint_count_drops_multiple_trailing_system_entries() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "user", "content": "hi"},
            {"role": "system", "content": "policy a"},
            {"role": "system", "content": "policy b"}
        ]
    });
    assert_eq!(fingerprint_message_count(&body), 1);
    Ok(())
}
