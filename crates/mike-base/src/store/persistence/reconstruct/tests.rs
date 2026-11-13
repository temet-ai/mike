use super::*;
use anyhow::Context;

const WIRE_BODY: &[u8] = br#"{"model":"claude-opus-5","messages":[{"role":"user","content":[{"type":"text","text":"first"}]},{"role":"assistant","content":[{"type":"text","text":"second"}]},{"role":"user","content":[{"type":"text","text":"third","cache_control":{"type":"ephemeral","ttl":"1h"}}]}],"system":[{"type":"text","text":"you are a helpful assistant"}],"tools":[{"name":"Bash","description":"run a command","input_schema":{"type":"object","properties":{"command":{"type":"string"}}}},{"name":"Read","description":"read a file","input_schema":{"type":"object","properties":{"path":{"type":"string"}}}}],"metadata":{"user_id":"u-1"},"max_tokens":64000,"thinking":{"type":"adaptive"},"context_management":{"edits":[{"type":"clear_thinking_20251015","keep":"all"}]},"output_config":{"effort":"xhigh"},"diagnostics":{"previous_message_id":"msg_01"},"stream":true}"#;

fn envelope_of(body: &[u8]) -> anyhow::Result<String> {
    let bytes = strip_bulk_arrays(body).context("body must capture an envelope")?;
    Ok(String::from_utf8(bytes)?)
}

#[test]
fn the_envelope_keeps_every_field_no_column_holds() -> anyhow::Result<()> {
    let text = envelope_of(WIRE_BODY)?;

    assert!(text.contains(r#""messages":[]"#), "conversation removed");
    assert!(text.contains(r#""tools":[]"#), "tool definitions removed");
    for fragment in [
        r#""metadata":{"user_id":"u-1"}"#,
        r#""context_management":{"edits":[{"type":"clear_thinking_20251015","keep":"all"}]}"#,
        r#""output_config":{"effort":"xhigh"}"#,
        r#""diagnostics":{"previous_message_id":"msg_01"}"#,
        r#""stream":true"#,
        r#""max_tokens":64000"#,
        r#""thinking":{"type":"adaptive"}"#,
        r#""model":"claude-opus-5""#,
        r#""system":[{"type":"text","text":"you are a helpful assistant"}]"#,
    ] {
        assert!(
            text.contains(fragment),
            "envelope must carry {fragment} verbatim, got: {text}"
        );
    }

    assert!(
        !text.contains(r#""role":"user""#),
        "no message content may survive in the envelope"
    );
    assert!(
        !text.contains("input_schema"),
        "no tool schema may survive in the envelope"
    );
    assert!(text.len() < WIRE_BODY.len(), "the envelope is smaller");
    Ok(())
}

#[test]
fn the_envelope_is_a_byte_exact_slice_of_the_original() -> anyhow::Result<()> {
    let envelope = envelope_of(WIRE_BODY)?;
    let original = std::str::from_utf8(WIRE_BODY)?;
    let tail = envelope
        .split(r#""tools":[]"#)
        .nth(1)
        .context("something follows the tools array")?;
    assert!(
        original.ends_with(tail),
        "the envelope's tail must be the original's tail, byte for byte"
    );
    assert!(
        original.starts_with(r#"{"model":"claude-opus-5","messages":["#),
        "sanity: the fixture starts where the test assumes"
    );
    Ok(())
}

#[test]
fn the_scanner_tolerates_whitespace_after_the_key() -> anyhow::Result<()> {
    let body = b"{\n    \"model\": \"claude-sonnet-4\",\n    \"messages\": [\n        {\"role\": \"user\"}\n    ],\n    \"max_tokens\": 16\n}";
    let text = envelope_of(body)?;
    assert!(
        !text.contains("\"role\""),
        "the conversation must be removed from a pretty-printed body too, got: {text}"
    );
    assert!(
        text.contains("\"max_tokens\": 16"),
        "and everything else kept verbatim, spacing included, got: {text}"
    );
    assert!(
        text.len() < body.len(),
        "a pretty-printed body must still shrink"
    );
    Ok(())
}

#[test]
fn a_non_object_body_captures_no_envelope() {
    assert_eq!(strip_bulk_arrays(b"[1,2,3]"), None);
    assert_eq!(strip_bulk_arrays(b"not json at all"), None);
    assert_eq!(strip_bulk_arrays(b""), None);
}

#[test]
fn the_scanner_ignores_a_nested_decoy_key() -> anyhow::Result<()> {
    let body = br#"{"model":"m","metadata":{"messages":[{"a":1}],"tools":[9]},"messages":[{"role":"user"}],"tools":[]}"#;
    let text = envelope_of(body)?;
    assert!(
        text.contains(r#""metadata":{"messages":[{"a":1}],"tools":[9]}"#),
        "the nested decoys must be untouched, got: {text}"
    );
    assert!(text.ends_with(r#""messages":[],"tools":[]}"#));
    Ok(())
}

#[test]
fn the_scanner_ignores_a_key_shaped_string_value() -> anyhow::Result<()> {
    let body = br#"{"note":"see \"messages\":[1] here","messages":[{"role":"user"}]}"#;
    let text = envelope_of(body)?;
    assert!(
        text.contains(r#""note":"see \"messages\":[1] here""#),
        "the quoted decoy must survive verbatim, got: {text}"
    );
    assert!(text.ends_with(r#""messages":[]}"#));
    Ok(())
}

#[test]
fn restore_cache_control_puts_the_breakpoint_back_where_it_was() -> anyhow::Result<()> {
    let mut message: JsonValue =
        serde_json::from_str(r#"{"role":"user","content":[{"type":"text","text":"hi"}]}"#)?;
    let recorded: JsonValue = serde_json::from_str(
        r#"[{"pointer":"/content/0/cache_control","value":{"ttl":"1h","type":"ephemeral"}}]"#,
    )?;
    restore_cache_control(&mut message, Some(&recorded));
    assert_eq!(
        message
            .pointer("/content/0/cache_control/ttl")
            .and_then(JsonValue::as_str),
        Some("1h")
    );
    Ok(())
}

#[test]
fn restore_cache_control_skips_a_pointer_with_no_parent() -> anyhow::Result<()> {
    let mut message: JsonValue = serde_json::from_str(r#"{"role":"user","content":[]}"#)?;
    let recorded: JsonValue = serde_json::from_str(
        r#"[{"pointer":"/content/7/cache_control","value":{"type":"ephemeral"}}]"#,
    )?;
    restore_cache_control(&mut message, Some(&recorded));
    assert_eq!(message["content"], serde_json::json!([]));
    Ok(())
}

#[test]
fn restore_cache_control_handles_the_message_level_breakpoint() -> anyhow::Result<()> {
    let mut message: JsonValue = serde_json::from_str(r#"{"role":"user"}"#)?;
    let recorded: JsonValue =
        serde_json::from_str(r#"[{"pointer":"/cache_control","value":{"type":"ephemeral"}}]"#)?;
    restore_cache_control(&mut message, Some(&recorded));
    assert_eq!(
        message
            .pointer("/cache_control/type")
            .and_then(JsonValue::as_str),
        Some("ephemeral")
    );
    Ok(())
}
