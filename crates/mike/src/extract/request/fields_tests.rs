use super::*;
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use serde_json::json;

#[test]
fn derive_session_id_uuid_header() -> anyhow::Result<()> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-claude-code-session-id",
        HeaderValue::from_static("550e8400-e29b-41d4-a716-446655440000"),
    );
    assert_eq!(
        derive_session_id(&headers),
        Ok("550e8400-e29b-41d4-a716-446655440000".to_string())
    );
    Ok(())
}

#[test]
fn derive_session_id_uppercase_header_lookup() -> anyhow::Result<()> {
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("x-claude-code-session-id"),
        HeaderValue::from_static("550e8400-e29b-41d4-a716-446655440000"),
    );
    assert!(headers.get("X-Claude-Code-Session-Id").is_some());
    assert_eq!(
        derive_session_id(&headers),
        Ok("550e8400-e29b-41d4-a716-446655440000".to_string())
    );
    Ok(())
}

#[test]
fn derive_session_id_trims_header() -> anyhow::Result<()> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-claude-code-session-id",
        HeaderValue::from_static("  550e8400-e29b-41d4-a716-446655440000  "),
    );
    assert_eq!(
        derive_session_id(&headers),
        Ok("550e8400-e29b-41d4-a716-446655440000".to_string())
    );
    Ok(())
}

#[test]
fn derive_session_id_accepts_uppercase_hex_uuid() -> anyhow::Result<()> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-claude-code-session-id",
        HeaderValue::from_static("550E8400-E29B-41D4-A716-446655440000"),
    );
    assert_eq!(
        derive_session_id(&headers),
        Ok("550E8400-E29B-41D4-A716-446655440000".to_string())
    );
    Ok(())
}

#[test]
fn derive_session_id_empty_header_returns_missing() -> anyhow::Result<()> {
    let mut headers = HeaderMap::new();
    headers.insert("x-claude-code-session-id", HeaderValue::from_static(""));
    assert_eq!(derive_session_id(&headers), Err(SessionIdMiss::Missing));
    Ok(())
}

#[test]
fn derive_session_id_whitespace_header_returns_missing() -> anyhow::Result<()> {
    let mut headers = HeaderMap::new();
    headers.insert("x-claude-code-session-id", HeaderValue::from_static("   "));
    assert_eq!(derive_session_id(&headers), Err(SessionIdMiss::Missing));
    Ok(())
}

#[test]
fn derive_session_id_non_utf8_header_returns_non_utf8() -> anyhow::Result<()> {
    let mut headers = HeaderMap::new();
    let raw = HeaderValue::from_bytes(&[0xFF, 0xFE, 0xFD])?;
    headers.insert("x-claude-code-session-id", raw);
    assert_eq!(derive_session_id(&headers), Err(SessionIdMiss::NonUtf8));
    Ok(())
}

#[test]
fn derive_session_id_non_uuid_header_returns_malformed() -> anyhow::Result<()> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-claude-code-session-id",
        HeaderValue::from_static("not-a-uuid"),
    );
    assert_eq!(derive_session_id(&headers), Err(SessionIdMiss::Malformed));
    Ok(())
}

#[test]
fn derive_session_id_wrong_hex_in_uuid_returns_malformed() -> anyhow::Result<()> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-claude-code-session-id",
        HeaderValue::from_static("550e84z0-e29b-41d4-a716-446655440000"),
    );
    assert_eq!(derive_session_id(&headers), Err(SessionIdMiss::Malformed));
    Ok(())
}

#[test]
fn derive_session_id_no_header_returns_missing() -> anyhow::Result<()> {
    let headers = HeaderMap::new();
    assert_eq!(derive_session_id(&headers), Err(SessionIdMiss::Missing));
    Ok(())
}

#[test]
fn model_present() -> anyhow::Result<()> {
    let body = json!({"model": "claude-sonnet-4-20250514"});
    assert_eq!(
        extract_model(&body),
        Some("claude-sonnet-4-20250514".into())
    );
    Ok(())
}

#[test]
fn model_missing() -> anyhow::Result<()> {
    let body = json!({});
    assert_eq!(extract_model(&body), None);
    Ok(())
}

#[test]
fn model_null() -> anyhow::Result<()> {
    let body = json!({"model": null});
    assert_eq!(extract_model(&body), None);
    Ok(())
}

#[test]
fn model_wrong_type() -> anyhow::Result<()> {
    let body = json!({"model": 42});
    assert_eq!(extract_model(&body), None);
    Ok(())
}

#[test]
fn stream_true() -> anyhow::Result<()> {
    let body = json!({"stream": true});
    assert!(extract_stream(&body));
    Ok(())
}

#[test]
fn stream_false() -> anyhow::Result<()> {
    let body = json!({"stream": false});
    assert!(!extract_stream(&body));
    Ok(())
}

#[test]
fn stream_missing_defaults_false() -> anyhow::Result<()> {
    let body = json!({});
    assert!(!extract_stream(&body));
    Ok(())
}

#[test]
fn stream_wrong_type_defaults_false() -> anyhow::Result<()> {
    let body = json!({"stream": "yes"});
    assert!(!extract_stream(&body));
    Ok(())
}

#[test]
fn system_prompt_string() -> anyhow::Result<()> {
    let body = json!({"system": "You are helpful."});
    assert_eq!(
        extract_system_prompt(&body),
        Some("You are helpful.".into())
    );
    Ok(())
}

#[test]
fn system_prompt_array_of_text_blocks() -> anyhow::Result<()> {
    let body = json!({
        "system": [
            {"type": "text", "text": "Line 1"},
            {"type": "text", "text": "Line 2"}
        ]
    });
    assert_eq!(extract_system_prompt(&body), Some("Line 1\nLine 2".into()));
    Ok(())
}

#[test]
fn system_prompt_empty_array() -> anyhow::Result<()> {
    let body = json!({"system": []});
    assert_eq!(extract_system_prompt(&body), None);
    Ok(())
}

#[test]
fn system_prompt_missing() -> anyhow::Result<()> {
    let body = json!({});
    assert_eq!(extract_system_prompt(&body), None);
    Ok(())
}

#[test]
fn system_prompt_mixed_block_types() -> anyhow::Result<()> {
    let body = json!({
        "system": [
            {"type": "text", "text": "Hello"},
            {"type": "image", "source": {}},
            {"type": "text", "text": " world"}
        ]
    });
    assert_eq!(extract_system_prompt(&body), Some("Hello\n world".into()));
    Ok(())
}

#[test]
fn system_prompt_null() -> anyhow::Result<()> {
    let body = json!({"system": null});
    assert_eq!(extract_system_prompt(&body), None);
    Ok(())
}

#[test]
fn system_prompt_wrong_type() -> anyhow::Result<()> {
    let body = json!({"system": 42});
    assert_eq!(extract_system_prompt(&body), None);
    Ok(())
}

#[test]
fn last_user_message_single() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "user", "content": "Hello"}
        ]
    });
    assert_eq!(extract_last_user_message(&body), "Hello");
    Ok(())
}

#[test]
fn last_user_message_multiple_picks_last() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "user", "content": "First"},
            {"role": "assistant", "content": "Reply"},
            {"role": "user", "content": "Second"}
        ]
    });
    assert_eq!(extract_last_user_message(&body), "Second");
    Ok(())
}

#[test]
fn last_user_message_no_user_messages() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "assistant", "content": "Hello"}
        ]
    });
    assert_eq!(extract_last_user_message(&body), "");
    Ok(())
}

#[test]
fn last_user_message_content_as_array() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "user", "content": [
                {"type": "text", "text": "Hello "},
                {"type": "text", "text": "world"}
            ]}
        ]
    });
    assert_eq!(extract_last_user_message(&body), "Hello world");
    Ok(())
}

#[test]
fn last_user_message_missing_messages() -> anyhow::Result<()> {
    let body = json!({});
    assert_eq!(extract_last_user_message(&body), "");
    Ok(())
}

#[test]
fn last_user_message_empty_messages() -> anyhow::Result<()> {
    let body = json!({"messages": []});
    assert_eq!(extract_last_user_message(&body), "");
    Ok(())
}

#[test]
fn last_user_message_with_tool_result_and_text() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "result"},
                {"type": "text", "text": "Now do this"}
            ]}
        ]
    });
    assert_eq!(extract_last_user_message(&body), "Now do this");
    Ok(())
}

#[test]
fn last_user_message_content_is_null() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "user", "content": null}
        ]
    });
    assert_eq!(extract_last_user_message(&body), "");
    Ok(())
}
