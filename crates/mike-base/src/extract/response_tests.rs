use super::*;
use serde_json::json;

#[test]
fn text_from_json_single_text_block() -> anyhow::Result<()> {
    let body = json!({
        "content": [{"type": "text", "text": "Hello"}]
    });
    assert_eq!(extract_text_from_json(&body), "Hello");
    Ok(())
}

#[test]
fn text_from_json_multiple_text_blocks_concatenate() -> anyhow::Result<()> {
    let body = json!({
        "content": [
            {"type": "text", "text": "Hello "},
            {"type": "text", "text": "world"}
        ]
    });
    assert_eq!(extract_text_from_json(&body), "Hello world");
    Ok(())
}

#[test]
fn text_from_json_ignores_tool_use_blocks() -> anyhow::Result<()> {
    let body = json!({
        "content": [
            {"type": "tool_use", "id": "t1", "name": "bash", "input": {}},
            {"type": "text", "text": "only text"}
        ]
    });
    assert_eq!(extract_text_from_json(&body), "only text");
    Ok(())
}

#[test]
fn text_from_json_empty_content_returns_empty() -> anyhow::Result<()> {
    let body = json!({"content": []});
    assert_eq!(extract_text_from_json(&body), "");
    Ok(())
}

#[test]
fn text_from_json_missing_content_returns_empty() -> anyhow::Result<()> {
    let body = json!({});
    assert_eq!(extract_text_from_json(&body), "");
    Ok(())
}

#[test]
fn accumulated_text_from_body_valid_json_with_text_returns_concat() -> anyhow::Result<()> {
    let body = br#"{"content":[{"type":"text","text":"hello"}]}"#;
    assert_eq!(extract_accumulated_text_from_body(body), "hello");
    Ok(())
}

#[test]
fn accumulated_text_from_body_valid_json_no_content_returns_empty() -> anyhow::Result<()> {
    let body = br#"{"id":"msg_1","type":"message"}"#;
    assert_eq!(extract_accumulated_text_from_body(body), "");
    Ok(())
}

#[test]
fn accumulated_text_from_body_malformed_json_returns_empty() -> anyhow::Result<()> {
    let body = b"not valid JSON at all";
    assert_eq!(extract_accumulated_text_from_body(body), "");
    Ok(())
}

#[test]
fn accumulated_text_from_body_empty_bytes_returns_empty() -> anyhow::Result<()> {
    assert_eq!(extract_accumulated_text_from_body(&[]), "");
    Ok(())
}

#[test]
fn accumulated_text_from_body_multiple_text_blocks_concatenate() -> anyhow::Result<()> {
    let body = br#"{"content":[{"type":"text","text":"foo "},{"type":"text","text":"bar"}]}"#;
    assert_eq!(extract_accumulated_text_from_body(body), "foo bar");
    Ok(())
}

#[test]
fn usage_message_start_normal() -> anyhow::Result<()> {
    let data = json!({
        "message": {
            "usage": {
                "input_tokens": 100,
                "output_tokens": 5,
                "cache_creation_input_tokens": 50,
                "cache_read_input_tokens": 25
            }
        }
    });
    let usage =
        extract_usage_from_message_start(&data).ok_or_else(|| anyhow::anyhow!("expected Some"))?;
    assert_eq!(usage.input_tokens, 100);
    assert_eq!(usage.output_tokens, 5);
    assert_eq!(usage.cache_creation_input_tokens, Some(50));
    assert_eq!(usage.cache_read_input_tokens, Some(25));
    Ok(())
}

#[test]
fn usage_message_start_missing_usage() -> anyhow::Result<()> {
    let data = json!({"message": {}});
    assert!(extract_usage_from_message_start(&data).is_none());
    Ok(())
}

#[test]
fn usage_message_start_missing_input_tokens() -> anyhow::Result<()> {
    let data = json!({
        "message": {
            "usage": {"output_tokens": 10}
        }
    });
    assert!(extract_usage_from_message_start(&data).is_none());
    Ok(())
}

#[test]
fn usage_message_start_no_cache_fields() -> anyhow::Result<()> {
    let data = json!({
        "message": {
            "usage": {
                "input_tokens": 50,
                "output_tokens": 0
            }
        }
    });
    let usage =
        extract_usage_from_message_start(&data).ok_or_else(|| anyhow::anyhow!("expected Some"))?;
    assert_eq!(usage.input_tokens, 50);
    assert_eq!(usage.output_tokens, 0);
    assert_eq!(usage.cache_creation_input_tokens, None);
    assert_eq!(usage.cache_read_input_tokens, None);
    Ok(())
}

#[test]
fn usage_message_start_output_tokens_missing_defaults_zero() -> anyhow::Result<()> {
    let data = json!({
        "message": {
            "usage": {"input_tokens": 50}
        }
    });
    let usage =
        extract_usage_from_message_start(&data).ok_or_else(|| anyhow::anyhow!("expected Some"))?;
    assert_eq!(usage.output_tokens, 0);
    Ok(())
}

#[test]
fn usage_message_start_no_message() -> anyhow::Result<()> {
    let data = json!({});
    assert!(extract_usage_from_message_start(&data).is_none());
    Ok(())
}

#[test]
fn usage_message_delta_normal() -> anyhow::Result<()> {
    let data = json!({"usage": {"output_tokens": 42}});
    let usage =
        extract_usage_from_message_delta(&data).ok_or_else(|| anyhow::anyhow!("expected Some"))?;
    assert_eq!(usage.input_tokens, 0);
    assert_eq!(usage.output_tokens, 42);
    assert_eq!(usage.cache_creation_input_tokens, None);
    assert_eq!(usage.cache_read_input_tokens, None);
    Ok(())
}

#[test]
fn usage_message_delta_missing() -> anyhow::Result<()> {
    let data = json!({});
    assert!(extract_usage_from_message_delta(&data).is_none());
    Ok(())
}

#[test]
fn usage_message_delta_missing_output_tokens() -> anyhow::Result<()> {
    let data = json!({"usage": {}});
    assert!(extract_usage_from_message_delta(&data).is_none());
    Ok(())
}

#[test]
fn usage_message_delta_wrong_type() -> anyhow::Result<()> {
    let data = json!({"usage": {"output_tokens": "not a number"}});
    assert!(extract_usage_from_message_delta(&data).is_none());
    Ok(())
}

#[test]
fn stop_reason_present() -> anyhow::Result<()> {
    let data = json!({"delta": {"stop_reason": "end_turn"}});
    assert_eq!(extract_stop_reason(&data), Some("end_turn".into()));
    Ok(())
}

#[test]
fn stop_reason_null() -> anyhow::Result<()> {
    let data = json!({"delta": {"stop_reason": null}});
    assert_eq!(extract_stop_reason(&data), None);
    Ok(())
}

#[test]
fn stop_reason_missing() -> anyhow::Result<()> {
    let data = json!({});
    assert_eq!(extract_stop_reason(&data), None);
    Ok(())
}

#[test]
fn stop_reason_tool_use() -> anyhow::Result<()> {
    let data = json!({"delta": {"stop_reason": "tool_use"}});
    assert_eq!(extract_stop_reason(&data), Some("tool_use".into()));
    Ok(())
}

#[test]
fn stop_details_present_in_delta() -> anyhow::Result<()> {
    let data = json!({
        "delta": {
            "stop_reason": "refusal",
            "stop_details": {"type": "refusal", "category": "violence"}
        }
    });
    assert_eq!(
        extract_stop_details(&data),
        Some(json!({"type": "refusal", "category": "violence"}))
    );
    Ok(())
}

#[test]
fn stop_details_present_top_level_final_message() -> anyhow::Result<()> {
    let data = json!({
        "stop_reason": "refusal",
        "stop_details": {"type": "refusal", "category": "self_harm"}
    });
    assert_eq!(
        extract_stop_details(&data),
        Some(json!({"type": "refusal", "category": "self_harm"}))
    );
    Ok(())
}

#[test]
fn stop_details_null() -> anyhow::Result<()> {
    let data = json!({"delta": {"stop_details": null}});
    assert_eq!(extract_stop_details(&data), None);
    Ok(())
}

#[test]
fn stop_details_missing() -> anyhow::Result<()> {
    let data = json!({"delta": {"stop_reason": "end_turn"}});
    assert_eq!(extract_stop_details(&data), None);
    Ok(())
}

#[test]
fn stop_details_empty_object() -> anyhow::Result<()> {
    let data = json!({});
    assert_eq!(extract_stop_details(&data), None);
    Ok(())
}

#[test]
fn stop_details_preserves_nested_shape_verbatim() -> anyhow::Result<()> {
    let detail = json!({
        "type": "refusal",
        "category": "weapons",
        "metadata": {"score": 0.97, "rationale": ["a", "b", "c"]}
    });
    let data = json!({"delta": {"stop_details": detail.clone()}});
    assert_eq!(extract_stop_details(&data), Some(detail));
    Ok(())
}

#[test]
fn text_delta_present() -> anyhow::Result<()> {
    let data = json!({"delta": {"type": "text_delta", "text": "Hello"}});
    assert_eq!(extract_text_delta(&data), Some("Hello".into()));
    Ok(())
}

#[test]
fn text_delta_missing() -> anyhow::Result<()> {
    let data = json!({"delta": {"type": "thinking_delta", "thinking": "hmm"}});
    assert_eq!(extract_text_delta(&data), None);
    Ok(())
}

#[test]
fn text_delta_empty_object() -> anyhow::Result<()> {
    let data = json!({});
    assert_eq!(extract_text_delta(&data), None);
    Ok(())
}

#[test]
fn thinking_delta_present() -> anyhow::Result<()> {
    let data = json!({"delta": {"type": "thinking_delta", "thinking": "Let me consider..."}});
    assert_eq!(
        extract_thinking_delta(&data),
        Some("Let me consider...".into())
    );
    Ok(())
}

#[test]
fn thinking_delta_missing() -> anyhow::Result<()> {
    let data = json!({"delta": {"type": "text_delta", "text": "Hello"}});
    assert_eq!(extract_thinking_delta(&data), None);
    Ok(())
}

#[test]
fn thinking_delta_empty_object() -> anyhow::Result<()> {
    let data = json!({});
    assert_eq!(extract_thinking_delta(&data), None);
    Ok(())
}

#[test]
fn signature_delta_present() -> anyhow::Result<()> {
    let data = json!({"delta": {"type": "signature_delta", "signature": "abc123sig=="}});
    assert_eq!(extract_signature_delta(&data), Some("abc123sig==".into()));
    Ok(())
}

#[test]
fn signature_delta_missing() -> anyhow::Result<()> {
    let data = json!({"delta": {"type": "thinking_delta", "thinking": "hmm"}});
    assert_eq!(extract_signature_delta(&data), None);
    Ok(())
}

#[test]
fn signature_delta_empty_object() -> anyhow::Result<()> {
    let data = json!({});
    assert_eq!(extract_signature_delta(&data), None);
    Ok(())
}

#[test]
fn tool_use_start_present() -> anyhow::Result<()> {
    let data = json!({
        "content_block": {
            "type": "tool_use",
            "id": "toolu_01",
            "name": "read_file"
        }
    });
    let (id, name) =
        extract_tool_use_start(&data).ok_or_else(|| anyhow::anyhow!("expected Some"))?;
    assert_eq!(id, "toolu_01");
    assert_eq!(name, "read_file");
    Ok(())
}

#[test]
fn tool_use_start_missing_id() -> anyhow::Result<()> {
    let data = json!({"content_block": {"type": "tool_use", "name": "bash"}});
    assert!(extract_tool_use_start(&data).is_none());
    Ok(())
}

#[test]
fn tool_use_start_missing_name() -> anyhow::Result<()> {
    let data = json!({"content_block": {"type": "tool_use", "id": "toolu_01"}});
    assert!(extract_tool_use_start(&data).is_none());
    Ok(())
}

#[test]
fn tool_use_start_missing_content_block() -> anyhow::Result<()> {
    let data = json!({});
    assert!(extract_tool_use_start(&data).is_none());
    Ok(())
}

#[test]
fn input_json_delta_present() -> anyhow::Result<()> {
    let data = json!({"delta": {"type": "input_json_delta", "partial_json": "{\"path\":"}});
    assert_eq!(extract_input_json_delta(&data), Some("{\"path\":".into()));
    Ok(())
}

#[test]
fn input_json_delta_missing() -> anyhow::Result<()> {
    let data = json!({"delta": {"type": "text_delta", "text": "Hello"}});
    assert_eq!(extract_input_json_delta(&data), None);
    Ok(())
}

#[test]
fn input_json_delta_empty_object() -> anyhow::Result<()> {
    let data = json!({});
    assert_eq!(extract_input_json_delta(&data), None);
    Ok(())
}

fn captured_message_start() -> serde_json::Value {
    json!({
        "type": "message_start",
        "message": {
            "id": "msg_01", "type": "message", "role": "assistant", "content": [],
            "model": "fable-5-1", "stop_reason": null, "stop_sequence": null,
            "usage": {
                "input_tokens": 2,
                "cache_creation_input_tokens": 29271,
                "cache_read_input_tokens": 13763,
                "cache_creation": {
                    "ephemeral_5m_input_tokens": 0,
                    "ephemeral_1h_input_tokens": 29271
                },
                "output_tokens": 1,
                "service_tier": "standard",
                "inference_geo": "not_available"
            },
            "input_transformations": [{"reason": "prefix_binding_mismatch"}]
        }
    })
}

#[test]
fn message_fields_start_and_delta_keep_wire_usage_and_extras() -> anyhow::Result<()> {
    let start = extract_message_start_fields(&captured_message_start())
        .ok_or_else(|| anyhow::anyhow!("message_start fields missing"))?;
    let delta = extract_message_delta_fields(&json!({
        "type": "message_delta",
        "delta": {"stop_reason": "stop_sequence", "stop_sequence": "</done>"},
        "usage": {"output_tokens": 77}
    }))
    .ok_or_else(|| anyhow::anyhow!("message_delta fields missing"))?;

    let mut merged = None;
    merge_message_fields(&mut merged, &start);
    merge_message_fields(&mut merged, &delta);
    let merged = merged.ok_or_else(|| anyhow::anyhow!("nothing merged"))?;
    let (usage, rest) = split_message_fields(&merged);
    let usage = usage.ok_or_else(|| anyhow::anyhow!("usage missing"))?;
    let rest = rest.ok_or_else(|| anyhow::anyhow!("extras missing"))?;

    assert_eq!(usage["cache_creation"]["ephemeral_1h_input_tokens"], 29271);
    assert_eq!(usage["service_tier"], "standard");
    assert_eq!(usage["output_tokens"], 77);
    assert_eq!(usage["input_tokens"], 2);
    assert_eq!(rest["stop_sequence"], "</done>");
    assert_eq!(
        rest["input_transformations"][0]["reason"],
        "prefix_binding_mismatch"
    );
    assert!(rest.get("usage").is_none());
    Ok(())
}

#[test]
fn message_fields_delta_without_stop_sequence_or_usage_is_none() {
    assert!(extract_message_delta_fields(&json!({"delta": {"stop_reason": "end_turn"}})).is_none());
}

#[test]
fn completion_from_buffered_body_reads_usage_and_stop_reason() -> anyhow::Result<()> {
    let body = serde_json::to_vec(&json!({
        "id": "msg_1", "type": "message", "stop_reason": "end_turn",
        "usage": {"input_tokens": 12, "output_tokens": 435,
                  "cache_creation_input_tokens": 7, "cache_read_input_tokens": 6234}
    }))?;
    let (usage, stop) = extract_completion_from_body(&body);
    let usage = usage.ok_or_else(|| anyhow::anyhow!("usage missing"))?;
    assert_eq!((usage.input_tokens, usage.output_tokens), (12, 435));
    assert_eq!(usage.cache_creation_input_tokens, Some(7));
    assert_eq!(usage.cache_read_input_tokens, Some(6234));
    assert_eq!(stop.as_deref(), Some("end_turn"));
    Ok(())
}

#[test]
fn completion_from_error_or_non_json_body_is_empty() {
    let (usage, stop) =
        extract_completion_from_body(br#"{"type":"error","error":{"type":"not_found_error"}}"#);
    assert!(usage.is_none() && stop.is_none());
    let (usage, stop) = extract_completion_from_body(b"not json");
    assert!(usage.is_none() && stop.is_none());
}
