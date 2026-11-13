use serde_json::Value;

use super::FIELD;
use super::strip::{SseStripper, strip_json_body};

fn event(data: &Value) -> String {
    format!(
        "event: {}\ndata: {data}\n\n",
        data["type"].as_str().unwrap_or_default()
    )
}

fn tool_stream(fragments: &[&str]) -> String {
    let mut out = event(&serde_json::json!({"type":"message_start","message":{"id":"m"}}));
    out += &event(
        &serde_json::json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
    );
    out += &event(
        &serde_json::json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Looking."}}),
    );
    out += &event(&serde_json::json!({"type":"content_block_stop","index":0}));
    out += &event(
        &serde_json::json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_1","name":"Read","input":{}}}),
    );
    for fragment in fragments {
        out += &event(
            &serde_json::json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":fragment}}),
        );
    }
    out += &event(&serde_json::json!({"type":"content_block_stop","index":1}));
    out += &event(&serde_json::json!({"type":"message_stop"}));
    out
}

fn relay_in_chunks(stream: &str, chunk: usize) -> String {
    let mut stripper = SseStripper::default();
    let mut out = Vec::new();
    for piece in stream.as_bytes().chunks(chunk) {
        out.extend(stripper.push(piece));
    }
    out.extend(stripper.finish());
    String::from_utf8(out).unwrap_or_default()
}

fn tool_input(stream: &str) -> Value {
    let joined: String = stream
        .split("\n\n")
        .filter_map(|e| e.lines().find_map(|l| l.strip_prefix("data: ")))
        .filter_map(|d| serde_json::from_str::<Value>(d).ok())
        .filter(|d| d["index"] == 1 && d["delta"]["type"] == "input_json_delta")
        .filter_map(|d| d["delta"]["partial_json"].as_str().map(str::to_owned))
        .collect();
    serde_json::from_str(&joined).unwrap_or(Value::Null)
}

#[test]
fn strip_stream_removes_the_field_split_across_fragments_and_chunks() {
    let stream = tool_stream(&[
        r#"{"file_path": "/a.rs", "context_fo"#,
        r#"cus_question": "where is the retry limit"}"#,
    ]);
    for chunk in [1, 7, 64, stream.len()] {
        let out = relay_in_chunks(&stream, chunk);
        assert_eq!(
            tool_input(&out),
            serde_json::json!({"file_path": "/a.rs"}),
            "chunk size {chunk}"
        );
        assert!(!out.contains(FIELD));
    }
}

#[test]
fn strip_stream_leaves_text_blocks_and_framing_events_byte_identical() {
    let stream = tool_stream(&[r#"{"file_path":"/a.rs","context_focus_question":"q"}"#]);
    let out = relay_in_chunks(&stream, 13);
    let text_part_end = stream.find("\"tool_use\"").unwrap_or_default();
    let prefix_end = stream[..text_part_end].rfind("event:").unwrap_or_default();
    assert!(out.starts_with(&stream[..prefix_end]));
    assert!(out.ends_with(&event(&serde_json::json!({"type":"message_stop"}))));
}

#[test]
fn strip_stream_without_tool_calls_is_byte_identical() {
    let mut stream = event(&serde_json::json!({"type":"message_start","message":{"id":"m"}}));
    stream += &event(
        &serde_json::json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
    );
    stream += &event(
        &serde_json::json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hi"}}),
    );
    stream += &event(&serde_json::json!({"type":"content_block_stop","index":0}));
    stream += "event: ping\ndata: {\"type\": \"ping\"}\n\n";
    assert_eq!(relay_in_chunks(&stream, 5), stream);
}

#[test]
fn strip_stream_with_unterminated_tail_passes_it_through() {
    let stream = "event: ping\ndata: {\"type\":\"ping\"}\n\nevent: partial\ndata: {";
    assert_eq!(relay_in_chunks(stream, 4), stream);
}

#[test]
fn strip_json_body_removes_the_field_from_tool_use_blocks() -> anyhow::Result<()> {
    let raw = br#"{"id":"m","content":[{"type":"text","text":"x"},{"type":"tool_use","id":"t","name":"Read","input":{"file_path":"/a","context_focus_question":"q"}}],"usage":{"output_tokens":3}}"#;
    let stripped = strip_json_body(raw).ok_or_else(|| anyhow::anyhow!("expected a strip"))?;
    let body: Value = serde_json::from_slice(&stripped)?;
    assert_eq!(
        body["content"][1]["input"],
        serde_json::json!({"file_path": "/a"})
    );
    assert!(String::from_utf8(stripped.to_vec())?.ends_with(r#","usage":{"output_tokens":3}}"#));
    Ok(())
}

#[test]
fn strip_json_body_without_the_field_returns_none() {
    assert!(strip_json_body(br#"{"content":[{"type":"text","text":"x"}]}"#).is_none());
    assert!(strip_json_body(b"not json").is_none());
}
