use serde_json::{Value, json};

use super::assistant_message_from_sse;

fn sse(events: &[Value]) -> String {
    events
        .iter()
        .map(|event| {
            let kind = event
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("message");
            format!("event: {kind}\ndata: {event}\n\n")
        })
        .collect()
}

fn message_start() -> Value {
    json!({
        "type": "message_start",
        "message": {
            "id": "msg_1",
            "type": "message",
            "role": "assistant",
            "model": "claude-test",
            "content": [],
            "stop_reason": null,
            "stop_sequence": null,
            "usage": {"input_tokens": 10, "output_tokens": 1}
        }
    })
}

fn block_start(index: usize, block: Value) -> Value {
    json!({"type": "content_block_start", "index": index, "content_block": block})
}

fn block_delta(index: usize, delta: Value) -> Value {
    json!({"type": "content_block_delta", "index": index, "delta": delta})
}

fn block_stop(index: usize) -> Value {
    json!({"type": "content_block_stop", "index": index})
}

fn text_delta(index: usize, text: &str) -> Value {
    block_delta(index, json!({"type": "text_delta", "text": text}))
}

fn json_delta(index: usize, fragment: &str) -> Value {
    block_delta(
        index,
        json!({"type": "input_json_delta", "partial_json": fragment}),
    )
}

fn tool_use_start(index: usize) -> Value {
    block_start(
        index,
        json!({"type": "tool_use", "id": "toolu_1", "name": "Read", "input": {}}),
    )
}

fn message_delta(stop_reason: &str, output_tokens: u64) -> Value {
    json!({
        "type": "message_delta",
        "delta": {"stop_reason": stop_reason, "stop_sequence": null},
        "usage": {"output_tokens": output_tokens}
    })
}

fn rebuild(stream: &str) -> anyhow::Result<Value> {
    assistant_message_from_sse(stream.as_bytes())
        .ok_or_else(|| anyhow::anyhow!("no message rebuilt from {stream:?}"))
}

#[test]
fn text_only_stream_rebuilds_single_text_block() -> anyhow::Result<()> {
    let stream = sse(&[
        message_start(),
        block_start(0, json!({"type": "text", "text": ""})),
        json!({"type": "ping"}),
        text_delta(0, "Hello"),
        text_delta(0, ", world"),
        block_stop(0),
        message_delta("end_turn", 5),
        json!({"type": "message_stop"}),
    ]);
    let message = rebuild(&stream)?;
    assert_eq!(message["id"], "msg_1");
    assert_eq!(message["role"], "assistant");
    assert_eq!(message["model"], "claude-test");
    assert_eq!(
        message["content"],
        json!([{"type": "text", "text": "Hello, world"}])
    );
    Ok(())
}

#[test]
fn text_and_tool_use_with_split_input_parses_input_object() -> anyhow::Result<()> {
    let stream = sse(&[
        message_start(),
        block_start(0, json!({"type": "text", "text": ""})),
        text_delta(0, "Reading."),
        block_stop(0),
        tool_use_start(1),
        json_delta(1, ""),
        json_delta(1, "{\"file_pa"),
        json_delta(1, "th\": \"/tmp/a"),
        json_delta(1, ".rs\", \"limit\": 20}"),
        block_stop(1),
        message_delta("tool_use", 30),
    ]);
    let message = rebuild(&stream)?;
    assert_eq!(
        message["content"],
        json!([
            {"type": "text", "text": "Reading."},
            {
                "type": "tool_use",
                "id": "toolu_1",
                "name": "Read",
                "input": {"file_path": "/tmp/a.rs", "limit": 20}
            }
        ])
    );
    assert_eq!(message["stop_reason"], "tool_use");
    Ok(())
}

#[test]
fn tool_use_without_input_deltas_keeps_empty_object() -> anyhow::Result<()> {
    let stream = sse(&[
        message_start(),
        tool_use_start(0),
        json_delta(0, ""),
        block_stop(0),
    ]);
    let message = rebuild(&stream)?;
    assert_eq!(message["content"][0]["input"], json!({}));
    Ok(())
}

#[test]
fn thinking_block_collects_text_and_signature() -> anyhow::Result<()> {
    let stream = sse(&[
        message_start(),
        block_start(
            0,
            json!({"type": "thinking", "thinking": "", "signature": ""}),
        ),
        block_delta(0, json!({"type": "thinking_delta", "thinking": "Let me "})),
        block_delta(0, json!({"type": "thinking_delta", "thinking": "think."})),
        block_delta(
            0,
            json!({"type": "signature_delta", "signature": "sig-abc"}),
        ),
        block_stop(0),
        block_start(1, json!({"type": "redacted_thinking", "data": "opaque"})),
        block_stop(1),
    ]);
    let message = rebuild(&stream)?;
    assert_eq!(
        message["content"],
        json!([
            {"type": "thinking", "thinking": "Let me think.", "signature": "sig-abc"},
            {"type": "redacted_thinking", "data": "opaque"}
        ])
    );
    Ok(())
}

#[test]
fn citations_delta_pushes_onto_citations_array() -> anyhow::Result<()> {
    let stream = sse(&[
        message_start(),
        block_start(0, json!({"type": "text", "text": "", "citations": null})),
        block_delta(
            0,
            json!({"type": "citations_delta", "citation": {"type": "web_search_result_location", "url": "u1"}}),
        ),
        block_delta(
            0,
            json!({"type": "citations_delta", "citation": {"type": "web_search_result_location", "url": "u2"}}),
        ),
    ]);
    let message = rebuild(&stream)?;
    assert_eq!(
        message["content"][0]["citations"],
        json!([
            {"type": "web_search_result_location", "url": "u1"},
            {"type": "web_search_result_location", "url": "u2"}
        ])
    );
    Ok(())
}

#[test]
fn message_delta_sets_stop_fields_and_merges_usage() -> anyhow::Result<()> {
    let stream = sse(&[
        message_start(),
        json!({
            "type": "message_delta",
            "delta": {"stop_reason": "stop_sequence", "stop_sequence": "END"},
            "usage": {"output_tokens": 42, "cache_read_input_tokens": 7, "input_tokens": null}
        }),
    ]);
    let message = rebuild(&stream)?;
    assert_eq!(message["stop_reason"], "stop_sequence");
    assert_eq!(message["stop_sequence"], "END");
    assert_eq!(
        message["usage"],
        json!({"input_tokens": 10, "output_tokens": 42, "cache_read_input_tokens": 7})
    );
    Ok(())
}

#[test]
fn crlf_line_endings_rebuild_same_message_as_lf() -> anyhow::Result<()> {
    let stream = sse(&[
        message_start(),
        block_start(0, json!({"type": "text", "text": ""})),
        text_delta(0, "Hi"),
        block_stop(0),
        message_delta("end_turn", 2),
    ]);
    let crlf = stream.replace('\n', "\r\n");
    assert_eq!(rebuild(&crlf)?, rebuild(&stream)?);
    assert_eq!(rebuild(&crlf)?["content"][0]["text"], "Hi");
    Ok(())
}

#[test]
fn data_line_without_space_after_colon_is_parsed() -> anyhow::Result<()> {
    let stream = format!("data:{}\n\n", message_start());
    let message = rebuild(&stream)?;
    assert_eq!(message["id"], "msg_1");
    Ok(())
}

#[test]
fn stream_cut_off_mid_tool_use_parses_complete_buffer() -> anyhow::Result<()> {
    let stream = sse(&[
        message_start(),
        tool_use_start(0),
        json_delta(0, "{\"command\": "),
        json_delta(0, "\"ls\"}"),
    ]);
    let message = rebuild(&stream)?;
    assert_eq!(message["content"][0]["input"], json!({"command": "ls"}));
    Ok(())
}

#[test]
fn stream_cut_off_mid_tool_use_with_truncated_json_keeps_empty_input() -> anyhow::Result<()> {
    let stream = sse(&[
        message_start(),
        tool_use_start(0),
        json_delta(0, "{\"command\": \"l"),
    ]);
    let message = rebuild(&stream)?;
    assert_eq!(message["content"][0]["input"], json!({}));
    assert_eq!(message["content"][0]["name"], "Read");
    Ok(())
}

#[test]
fn stream_cut_off_mid_frame_keeps_blocks_assembled_so_far() -> anyhow::Result<()> {
    let mut stream = sse(&[
        message_start(),
        block_start(0, json!({"type": "text", "text": ""})),
        text_delta(0, "partial"),
    ]);
    stream.push_str("event: content_block_delta\ndata: {\"type\": \"content_blo");
    let message = rebuild(&stream)?;
    assert_eq!(message["content"][0]["text"], "partial");
    Ok(())
}

#[test]
fn unknown_block_and_delta_types_are_kept_or_ignored() -> anyhow::Result<()> {
    let stream = sse(&[
        message_start(),
        block_start(
            0,
            json!({"type": "server_tool_use", "id": "srv_1", "name": "web_search", "input": {}}),
        ),
        json_delta(0, "{\"query\": \"rust\"}"),
        block_stop(0),
        block_start(1, json!({"type": "future_block", "payload": 1})),
        block_delta(1, json!({"type": "future_delta", "x": 1})),
        block_stop(1),
    ]);
    let message = rebuild(&stream)?;
    assert_eq!(
        message["content"],
        json!([
            {"type": "server_tool_use", "id": "srv_1", "name": "web_search", "input": {"query": "rust"}},
            {"type": "future_block", "payload": 1}
        ])
    );
    Ok(())
}

#[test]
fn comments_and_unparseable_frames_are_ignored() -> anyhow::Result<()> {
    let stream = format!(
        ": keep-alive\n\ndata: not json\n\n{}data: [DONE]\n\n",
        sse(&[
            message_start(),
            block_start(0, json!({"type": "text", "text": "x"}))
        ])
    );
    let message = rebuild(&stream)?;
    assert_eq!(message["content"], json!([{"type": "text", "text": "x"}]));
    Ok(())
}

#[test]
fn stream_without_message_start_returns_none() {
    let stream = sse(&[
        block_start(0, json!({"type": "text", "text": ""})),
        text_delta(0, "orphan"),
        message_delta("end_turn", 1),
    ]);
    assert!(assistant_message_from_sse(stream.as_bytes()).is_none());
}

#[test]
fn garbage_input_returns_none() {
    assert!(assistant_message_from_sse(b"").is_none());
    assert!(assistant_message_from_sse(b"hello\nworld\n\n").is_none());
    assert!(assistant_message_from_sse(b"data: {\"type\": \"message_start\"").is_none());
    assert!(assistant_message_from_sse(&[0xff, 0xfe, 0x00, b'\n', 0x80, b'd']).is_none());
}
