use super::*;
use crate::store::models::RequestId;
use crate::types::TokenUsage;
use serde_json::{Value, json};

fn event_to_json(event: &ProxyEvent) -> anyhow::Result<Value> {
    Ok(serde_json::from_str(&serde_json::to_string(event)?)?)
}

#[test]
fn model_confirmed_serializes_with_type_tag() -> anyhow::Result<()> {
    let event = ProxyEvent::ModelConfirmed {
        id: RequestId::from_raw(42),
        model: "claude-opus-4-20250514".to_string(),
    };
    assert_eq!(
        event_to_json(&event)?,
        json!({
            "type": "ModelConfirmed",
            "id": 42,
            "model": "claude-opus-4-20250514"
        })
    );
    Ok(())
}

#[test]
fn text_delta_serializes_with_type_tag() -> anyhow::Result<()> {
    let event = ProxyEvent::TextDelta {
        id: RequestId::from_raw(7),
        text: "hello".to_string(),
    };
    assert_eq!(
        event_to_json(&event)?,
        json!({
            "type": "TextDelta",
            "id": 7,
            "text": "hello"
        })
    );
    Ok(())
}

#[test]
fn thinking_delta_serializes_with_type_tag() -> anyhow::Result<()> {
    let event = ProxyEvent::ThinkingDelta {
        id: RequestId::from_raw(3),
        thinking: "step 1".to_string(),
    };
    assert_eq!(
        event_to_json(&event)?,
        json!({
            "type": "ThinkingDelta",
            "id": 3,
            "thinking": "step 1"
        })
    );
    Ok(())
}

#[test]
fn tool_use_start_serializes_with_type_tag() -> anyhow::Result<()> {
    let event = ProxyEvent::ToolUseStart {
        id: RequestId::from_raw(11),
        tool_id: "toolu_1".to_string(),
        tool_name: "bash".to_string(),
    };
    assert_eq!(
        event_to_json(&event)?,
        json!({
            "type": "ToolUseStart",
            "id": 11,
            "tool_id": "toolu_1",
            "tool_name": "bash"
        })
    );
    Ok(())
}

#[test]
fn tool_input_delta_serializes_with_type_tag() -> anyhow::Result<()> {
    let event = ProxyEvent::ToolInputDelta {
        id: RequestId::from_raw(11),
        partial_json: "{\"cmd\":\"ls\"}".to_string(),
    };
    assert_eq!(
        event_to_json(&event)?,
        json!({
            "type": "ToolInputDelta",
            "id": 11,
            "partial_json": "{\"cmd\":\"ls\"}"
        })
    );
    Ok(())
}

#[test]
fn tool_use_end_serializes_with_type_tag() -> anyhow::Result<()> {
    let event = ProxyEvent::ToolUseEnd {
        id: RequestId::from_raw(11),
    };
    assert_eq!(
        event_to_json(&event)?,
        json!({
            "type": "ToolUseEnd",
            "id": 11
        })
    );
    Ok(())
}

#[test]
fn response_complete_serializes_with_type_tag() -> anyhow::Result<()> {
    let event = ProxyEvent::ResponseComplete {
        id: RequestId::from_raw(42),
        stop_reason: Some("end_turn".to_string()),
        stop_details: None,
        usage: Some(TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
        }),
    };
    assert_eq!(
        event_to_json(&event)?,
        json!({
            "type": "ResponseComplete",
            "id": 42,
            "stop_reason": "end_turn",
            "stop_details": null,
            "usage": {
                "input_tokens": 100,
                "output_tokens": 50,
                "cache_creation_input_tokens": null,
                "cache_read_input_tokens": null
            }
        })
    );
    Ok(())
}

#[test]
fn response_error_serializes_with_type_tag() -> anyhow::Result<()> {
    let event = ProxyEvent::ResponseError {
        id: RequestId::from_raw(42),
        error: "overloaded_error".to_string(),
    };
    assert_eq!(
        event_to_json(&event)?,
        json!({
            "type": "ResponseError",
            "id": 42,
            "error": "overloaded_error"
        })
    );
    Ok(())
}

#[test]
fn usage_received_serializes_with_type_tag() -> anyhow::Result<()> {
    let event = ProxyEvent::UsageReceived {
        id: RequestId::from_raw(42),
        usage: TokenUsage {
            input_tokens: 100,
            output_tokens: 0,
            cache_creation_input_tokens: Some(50),
            cache_read_input_tokens: Some(25),
        },
    };
    assert_eq!(
        event_to_json(&event)?,
        json!({
            "type": "UsageReceived",
            "id": 42,
            "usage": {
                "input_tokens": 100,
                "output_tokens": 0,
                "cache_creation_input_tokens": 50,
                "cache_read_input_tokens": 25
            }
        })
    );
    Ok(())
}

#[test]
fn lag_frame_serializes_with_type_tag() -> anyhow::Result<()> {
    let frame = LagFrame::new(17);
    let value: Value = serde_json::from_str(&serde_json::to_string(&frame)?)?;
    assert_eq!(
        value,
        json!({
            "type": "Lagged",
            "skipped": 17
        })
    );
    Ok(())
}

#[test]
fn row_changed_requests_insert_serializes_with_session_id() -> anyhow::Result<()> {
    let event = ProxyEvent::RowChanged {
        table: crate::store::models::RowChangedTable::Requests,
        id: "42".to_string(),
        session_id: Some("abcd".to_string()),
        op: crate::store::models::RowChangedOp::Insert,
    };
    assert_eq!(
        event_to_json(&event)?,
        json!({
            "type": "RowChanged",
            "table": "requests",
            "id": "42",
            "session_id": "abcd",
            "op": "INSERT"
        })
    );
    Ok(())
}

#[test]
fn row_changed_sessions_update_serializes_without_session_id() -> anyhow::Result<()> {
    let event = ProxyEvent::RowChanged {
        table: crate::store::models::RowChangedTable::Sessions,
        id: "session-xyz".to_string(),
        session_id: None,
        op: crate::store::models::RowChangedOp::Update,
    };
    assert_eq!(
        event_to_json(&event)?,
        json!({
            "type": "RowChanged",
            "table": "sessions",
            "id": "session-xyz",
            "session_id": null,
            "op": "UPDATE"
        })
    );
    Ok(())
}

#[test]
fn row_changed_filter_rules_delete_serializes_with_uppercase_op() -> anyhow::Result<()> {
    let event = ProxyEvent::RowChanged {
        table: crate::store::models::RowChangedTable::FilterRules,
        id: "11111111-2222-3333-4444-555555555555".to_string(),
        session_id: None,
        op: crate::store::models::RowChangedOp::Delete,
    };
    assert_eq!(
        event_to_json(&event)?,
        json!({
            "type": "RowChanged",
            "table": "filter_rules",
            "id": "11111111-2222-3333-4444-555555555555",
            "session_id": null,
            "op": "DELETE"
        })
    );
    Ok(())
}

#[test]
fn row_changed_prompt_compositions_insert_serializes_with_snake_case_table() -> anyhow::Result<()> {
    let event = ProxyEvent::RowChanged {
        table: crate::store::models::RowChangedTable::PromptCompositions,
        id: "deadbeef-cafe-0000-0000-000000000001".to_string(),
        session_id: None,
        op: crate::store::models::RowChangedOp::Insert,
    };
    assert_eq!(
        event_to_json(&event)?,
        json!({
            "type": "RowChanged",
            "table": "prompt_compositions",
            "id": "deadbeef-cafe-0000-0000-000000000001",
            "session_id": null,
            "op": "INSERT"
        })
    );
    Ok(())
}

#[test]
fn row_changed_mailbox_messages_insert_serializes_with_session_id() -> anyhow::Result<()> {
    let event = ProxyEvent::RowChanged {
        table: crate::store::models::RowChangedTable::MailboxMessages,
        id: "deadbeef-cafe-0000-0000-000000000002".to_string(),
        session_id: Some("recipient-session".to_string()),
        op: crate::store::models::RowChangedOp::Insert,
    };
    assert_eq!(
        event_to_json(&event)?,
        json!({
            "type": "RowChanged",
            "table": "mailbox_messages",
            "id": "deadbeef-cafe-0000-0000-000000000002",
            "session_id": "recipient-session",
            "op": "INSERT"
        })
    );
    Ok(())
}

#[test]
fn row_changed_mailbox_threads_update_serializes_without_session_id() -> anyhow::Result<()> {
    let event = ProxyEvent::RowChanged {
        table: crate::store::models::RowChangedTable::MailboxThreads,
        id: "deadbeef-cafe-0000-0000-000000000003".to_string(),
        session_id: None,
        op: crate::store::models::RowChangedOp::Update,
    };
    assert_eq!(
        event_to_json(&event)?,
        json!({
            "type": "RowChanged",
            "table": "mailbox_threads",
            "id": "deadbeef-cafe-0000-0000-000000000003",
            "session_id": null,
            "op": "UPDATE"
        })
    );
    Ok(())
}

#[path = "ws_tests_new_events.rs"]
mod new_events;
