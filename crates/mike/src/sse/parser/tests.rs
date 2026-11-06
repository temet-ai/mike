use super::*;

fn sse_bytes(event_type: &str, data: &str) -> Bytes {
    Bytes::from(format!("event: {event_type}\ndata: {data}\n\n"))
}

async fn run_parser_with_events(chunks: Vec<Bytes>) -> anyhow::Result<Vec<ProxyEvent>> {
    let all = run_parser_all_events(chunks).await?;
    Ok(all
        .into_iter()
        .filter(|e| !matches!(e, ProxyEvent::MessageFieldsReceived { .. }))
        .collect())
}

#[tokio::test]
async fn test_message_fields_event_carries_stop_sequence_and_usage() -> anyhow::Result<()> {
    let start = r#"{"type":"message_start","message":{"id":"msg_01","type":"message","role":"assistant","content":[],"model":"m","stop_reason":null,"input_transformations":[{"kind":"x"}],"usage":{"input_tokens":1,"output_tokens":0}}}"#;
    let delta = r#"{"type":"message_delta","delta":{"stop_reason":"stop_sequence","stop_sequence":"END"},"usage":{"output_tokens":4,"cache_creation":{"ephemeral_5m_input_tokens":9}}}"#;

    let events = run_parser_all_events(vec![
        sse_bytes("message_start", start),
        sse_bytes("message_delta", delta),
    ])
    .await?;

    let fields: Vec<&serde_json::Value> = events
        .iter()
        .filter_map(|e| match e {
            ProxyEvent::MessageFieldsReceived { fields, .. } => Some(fields),
            _ => None,
        })
        .collect();
    assert_eq!(fields.len(), 2);
    assert_eq!(
        fields[0].pointer("/input_transformations/0/kind"),
        Some(&serde_json::json!("x"))
    );
    assert_eq!(
        fields[1].pointer("/stop_sequence"),
        Some(&serde_json::json!("END"))
    );
    assert_eq!(
        fields[1].pointer("/usage/cache_creation/ephemeral_5m_input_tokens"),
        Some(&serde_json::json!(9))
    );
    Ok(())
}

async fn run_parser_all_events(chunks: Vec<Bytes>) -> anyhow::Result<Vec<ProxyEvent>> {
    let (parser_tx, parser_rx) = mpsc::channel::<Bytes>(64);
    let (event_tx, _event_rx) = broadcast::channel::<ProxyEvent>(64);
    let (store_tx, mut store_rx) = mpsc::channel::<ProxyEvent>(128);
    let request_id = RequestId::from_raw(1);

    for chunk in chunks {
        parser_tx.send(chunk).await?;
    }
    drop(parser_tx);

    run_parser(parser_rx, request_id, event_tx, store_tx).await;

    let mut events = Vec::new();
    while let Ok(ev) = store_rx.try_recv() {
        events.push(ev);
    }
    Ok(events)
}

#[tokio::test]
async fn test_message_start_extracts_model_and_usage() -> anyhow::Result<()> {
    let data = r#"{"type":"message_start","message":{"id":"msg_01","type":"message","role":"assistant","content":[],"model":"claude-sonnet-4-20250514","stop_reason":null,"usage":{"input_tokens":25,"output_tokens":0}}}"#;
    let events = run_parser_with_events(vec![sse_bytes("message_start", data)]).await?;

    assert_eq!(events.len(), 3);
    match &events[0] {
        ProxyEvent::UsageReceived { usage, .. } => {
            assert_eq!(usage.input_tokens, 25);
            assert_eq!(usage.output_tokens, 0);
        }
        other => anyhow::bail!("expected UsageReceived, got: {other:?}"),
    }
    match &events[1] {
        ProxyEvent::ModelConfirmed { model, .. } => {
            assert_eq!(model, "claude-sonnet-4-20250514");
        }
        other => anyhow::bail!("expected ModelConfirmed, got: {other:?}"),
    }
    match &events[2] {
        ProxyEvent::UpstreamMessageIdReceived {
            upstream_message_id,
            ..
        } => {
            assert_eq!(upstream_message_id, "msg_01");
        }
        other => anyhow::bail!("expected UpstreamMessageIdReceived, got: {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn test_message_start_without_id_skips_upstream_message_event() -> anyhow::Result<()> {
    let data = r#"{"type":"message_start","message":{"type":"message","role":"assistant","content":[],"model":"claude-sonnet-4-20250514","stop_reason":null,"usage":{"input_tokens":1,"output_tokens":0}}}"#;
    let events = run_parser_with_events(vec![sse_bytes("message_start", data)]).await?;

    for ev in &events {
        if let ProxyEvent::UpstreamMessageIdReceived { .. } = ev {
            anyhow::bail!("UpstreamMessageIdReceived must not be emitted without an id");
        }
    }
    Ok(())
}

#[tokio::test]
async fn test_content_block_delta_text() -> anyhow::Result<()> {
    let block_start =
        r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#;
    let delta =
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;

    let events = run_parser_with_events(vec![
        sse_bytes("content_block_start", block_start),
        sse_bytes("content_block_delta", delta),
    ])
    .await?;

    assert_eq!(events.len(), 1);
    match &events[0] {
        ProxyEvent::TextDelta { text, .. } => {
            assert_eq!(text, "Hello");
        }
        other => anyhow::bail!("expected TextDelta, got: {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn test_thinking_delta_stored_separately() -> anyhow::Result<()> {
    let block_start = r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#;
    let delta = r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Let me think..."}}"#;

    let events = run_parser_with_events(vec![
        sse_bytes("content_block_start", block_start),
        sse_bytes("content_block_delta", delta),
    ])
    .await?;

    assert_eq!(events.len(), 1);
    match &events[0] {
        ProxyEvent::ThinkingDelta { thinking, .. } => {
            assert_eq!(thinking, "Let me think...");
        }
        other => anyhow::bail!("expected ThinkingDelta, got: {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn test_signature_delta_produces_signature_event() -> anyhow::Result<()> {
    let block_start = r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#;
    let thinking = r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"step"}}"#;
    let sig = r#"{"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig-abc=="}}"#;

    let events = run_parser_with_events(vec![
        sse_bytes("content_block_start", block_start),
        sse_bytes("content_block_delta", thinking),
        sse_bytes("content_block_delta", sig),
    ])
    .await?;

    assert_eq!(events.len(), 2);
    match &events[0] {
        ProxyEvent::ThinkingDelta { thinking, .. } => assert_eq!(thinking, "step"),
        other => anyhow::bail!("expected ThinkingDelta, got: {other:?}"),
    }
    match &events[1] {
        ProxyEvent::SignatureDelta { signature, .. } => assert_eq!(signature, "sig-abc=="),
        other => anyhow::bail!("expected SignatureDelta, got: {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn test_tool_use_events_tracked() -> anyhow::Result<()> {
    let block_start = r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_01","name":"read_file"}}"#;
    let delta = r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"path\""}}"#;
    let block_stop = r#"{"type":"content_block_stop","index":1}"#;

    let events = run_parser_with_events(vec![
        sse_bytes("content_block_start", block_start),
        sse_bytes("content_block_delta", delta),
        sse_bytes("content_block_stop", block_stop),
    ])
    .await?;

    assert_eq!(events.len(), 3);

    match &events[0] {
        ProxyEvent::ToolUseStart {
            tool_id, tool_name, ..
        } => {
            assert_eq!(tool_id, "toolu_01");
            assert_eq!(tool_name, "read_file");
        }
        other => anyhow::bail!("expected ToolUseStart, got: {other:?}"),
    }

    match &events[1] {
        ProxyEvent::ToolInputDelta { partial_json, .. } => {
            assert_eq!(partial_json, r#"{"path""#);
        }
        other => anyhow::bail!("expected ToolInputDelta, got: {other:?}"),
    }

    match &events[2] {
        ProxyEvent::ToolUseEnd { .. } => {}
        other => anyhow::bail!("expected ToolUseEnd, got: {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn test_message_delta_captures_stop_reason() -> anyhow::Result<()> {
    let data = r#"{"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":42}}"#;

    let events = run_parser_with_events(vec![sse_bytes("message_delta", data)]).await?;

    assert_eq!(events.len(), 1);
    match &events[0] {
        ProxyEvent::ResponseComplete {
            stop_reason, usage, ..
        } => {
            assert_eq!(stop_reason.as_deref(), Some("end_turn"));
            let u = usage
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("usage should be present"))?;
            assert_eq!(u.output_tokens, 42);
        }
        other => anyhow::bail!("expected ResponseComplete, got: {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn test_message_delta_captures_refusal_stop_details() -> anyhow::Result<()> {
    let data = r#"{"type":"message_delta","delta":{"stop_reason":"refusal","stop_details":{"type":"refusal","category":"violence"}},"usage":{"output_tokens":3}}"#;

    let events = run_parser_with_events(vec![sse_bytes("message_delta", data)]).await?;

    assert_eq!(events.len(), 1);
    match &events[0] {
        ProxyEvent::ResponseComplete {
            stop_reason,
            stop_details,
            ..
        } => {
            assert_eq!(stop_reason.as_deref(), Some("refusal"));
            assert_eq!(
                stop_details.as_ref(),
                Some(&serde_json::json!({"type": "refusal", "category": "violence"}))
            );
        }
        other => anyhow::bail!("expected ResponseComplete, got: {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn test_message_delta_no_stop_details_is_none() -> anyhow::Result<()> {
    let data = r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":7}}"#;

    let events = run_parser_with_events(vec![sse_bytes("message_delta", data)]).await?;

    assert_eq!(events.len(), 1);
    match &events[0] {
        ProxyEvent::ResponseComplete { stop_details, .. } => {
            assert_eq!(stop_details.as_ref(), None);
        }
        other => anyhow::bail!("expected ResponseComplete, got: {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn test_error_event_published() -> anyhow::Result<()> {
    let data = r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;

    let events = run_parser_with_events(vec![sse_bytes("error", data)]).await?;

    assert_eq!(events.len(), 1);
    match &events[0] {
        ProxyEvent::ResponseError { error, .. } => {
            assert_eq!(error, "Overloaded");
        }
        other => anyhow::bail!("expected ResponseError, got: {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn test_malformed_json_skipped() -> anyhow::Result<()> {
    let bad_data = "not valid json{{{";
    let good_data = r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":10}}"#;

    let events = run_parser_with_events(vec![
        sse_bytes("message_start", bad_data),
        sse_bytes("message_delta", good_data),
    ])
    .await?;

    assert_eq!(events.len(), 1);
    match &events[0] {
        ProxyEvent::ResponseComplete { stop_reason, .. } => {
            assert_eq!(stop_reason.as_deref(), Some("end_turn"));
        }
        other => anyhow::bail!("expected ResponseComplete, got: {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn test_content_block_stop_resets_block_type() -> anyhow::Result<()> {
    let tool_start = r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_01","name":"bash"}}"#;
    let tool_stop = r#"{"type":"content_block_stop","index":0}"#;
    let text_start =
        r#"{"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}"#;
    let text_stop = r#"{"type":"content_block_stop","index":1}"#;

    let events = run_parser_with_events(vec![
        sse_bytes("content_block_start", tool_start),
        sse_bytes("content_block_stop", tool_stop),
        sse_bytes("content_block_start", text_start),
        sse_bytes("content_block_stop", text_stop),
    ])
    .await?;

    assert_eq!(events.len(), 2);
    match &events[0] {
        ProxyEvent::ToolUseStart { .. } => {}
        other => anyhow::bail!("expected ToolUseStart, got: {other:?}"),
    }
    match &events[1] {
        ProxyEvent::ToolUseEnd { .. } => {}
        other => anyhow::bail!("expected ToolUseEnd, got: {other:?}"),
    }
    Ok(())
}
