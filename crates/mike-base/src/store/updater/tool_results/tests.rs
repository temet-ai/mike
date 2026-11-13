use super::*;

#[test]
fn extract_tool_results_string_content_is_preserved_verbatim() -> anyhow::Result<()> {
    let body = serde_json::json!({
        "messages": [
            {"role": "user", "content": [
                {"type": "tool_result",
                 "tool_use_id": "toolu_01",
                 "is_error": false,
                 "content": "first 10 lines:\n1\n2\n3"}
            ]}
        ]
    });
    let results = extract_tool_results_from_body(&body);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].tool_use_id, "toolu_01");
    assert!(!results[0].is_error);
    assert_eq!(
        results[0].content,
        serde_json::Value::String("first 10 lines:\n1\n2\n3".to_string())
    );
    assert_eq!(
        results[0].content_bytes,
        results[0].content.to_string().len() as u64
    );
    assert_eq!(results[0].content_sha256.len(), 64);
    Ok(())
}

#[test]
fn extract_tool_results_array_content_is_preserved_verbatim() -> anyhow::Result<()> {
    let body = serde_json::json!({
        "messages": [
            {"role": "user", "content": [
                {"type": "tool_result",
                 "tool_use_id": "toolu_02",
                 "is_error": false,
                 "content": [
                     {"type": "text", "text": "the first 5 results are"},
                     {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "iVBORw0KG..."}}
                 ]}
            ]}
        ]
    });
    let results = extract_tool_results_from_body(&body);
    assert_eq!(results.len(), 1);
    let arr = results[0]
        .content
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("content must be an array"))?;
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["type"], "text");
    assert_eq!(arr[0]["text"], "the first 5 results are");
    assert_eq!(arr[1]["type"], "image");
    assert_eq!(arr[1]["source"]["data"], "iVBORw0KG...");
    Ok(())
}

#[test]
fn extract_tool_results_is_error_propagates() -> anyhow::Result<()> {
    let body = serde_json::json!({
        "messages": [
            {"role": "user", "content": [
                {"type": "tool_result",
                 "tool_use_id": "toolu_err",
                 "is_error": true,
                 "content": "error: command failed"}
            ]}
        ]
    });
    let results = extract_tool_results_from_body(&body);
    assert_eq!(results.len(), 1);
    assert!(
        results[0].is_error,
        "is_error: true on the wire must propagate to the extracted record"
    );
    Ok(())
}

#[test]
fn extract_tool_results_is_error_defaults_false_when_absent() -> anyhow::Result<()> {
    let body = serde_json::json!({
        "messages": [
            {"role": "user", "content": [
                {"type": "tool_result",
                 "tool_use_id": "toolu_03",
                 "content": "ok"}
            ]}
        ]
    });
    let results = extract_tool_results_from_body(&body);
    assert_eq!(results.len(), 1);
    assert!(!results[0].is_error);
    Ok(())
}

#[test]
fn extract_tool_results_skips_blocks_without_tool_use_id() -> anyhow::Result<()> {
    let body = serde_json::json!({
        "messages": [
            {"role": "user", "content": [
                {"type": "tool_result",
                 "is_error": false,
                 "content": "no anchor"},
                {"type": "tool_result",
                 "tool_use_id": "toolu_real",
                 "content": "ok"}
            ]}
        ]
    });
    let results = extract_tool_results_from_body(&body);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].tool_use_id, "toolu_real");
    Ok(())
}

#[test]
fn extract_tool_results_returns_empty_when_no_tool_result_blocks() {
    let body = serde_json::json!({
        "messages": [
            {"role": "user", "content": [
                {"type": "text", "text": "hello"}
            ]}
        ]
    });
    let results = extract_tool_results_from_body(&body);
    assert!(results.is_empty());
}

#[test]
fn extract_tool_results_returns_empty_when_messages_missing() {
    let body = serde_json::json!({"foo": "bar"});
    let results = extract_tool_results_from_body(&body);
    assert!(results.is_empty());
}

#[test]
fn extract_tool_results_handles_string_content_field() {
    let body = serde_json::json!({
        "messages": [
            {"role": "user", "content": "just a string with type tool_result inside"}
        ]
    });
    let results = extract_tool_results_from_body(&body);
    assert!(results.is_empty());
}

#[test]
fn extract_tool_results_collects_multiple_blocks_in_order() -> anyhow::Result<()> {
    let body = serde_json::json!({
        "messages": [
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "a", "content": "1"},
                {"type": "tool_result", "tool_use_id": "b", "content": "2"}
            ]},
            {"role": "assistant", "content": [{"type": "text", "text": "ok"}]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "c", "content": "3"}
            ]}
        ]
    });
    let results = extract_tool_results_from_body(&body);
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].tool_use_id, "a");
    assert_eq!(results[1].tool_use_id, "b");
    assert_eq!(results[2].tool_use_id, "c");
    Ok(())
}

#[test]
fn extract_tool_results_content_sha256_is_stable_for_same_value() {
    let body = serde_json::json!({
        "messages": [
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "a", "content": "x"},
                {"type": "tool_result", "tool_use_id": "b", "content": "x"}
            ]}
        ]
    });
    let results = extract_tool_results_from_body(&body);
    assert_eq!(results.len(), 2);
    assert_eq!(
        results[0].content_sha256, results[1].content_sha256,
        "identical content must produce identical sha256"
    );
}
