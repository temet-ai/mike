use serde_json::json;

pub(super) fn truncated_sse_body(chunks: &[&str]) -> String {
    let mut body = String::new();
    body.push_str("event: message_start\ndata: ");
    body.push_str(
        &json!({
            "type": "message_start",
            "message": {
                "id": "msg_abort_test",
                "type": "message",
                "role": "assistant",
                "content": [],
                "model": "claude-sonnet-4-20250514",
                "stop_reason": null,
                "usage": {"input_tokens": 10, "output_tokens": 0}
            }
        })
        .to_string(),
    );
    body.push_str("\n\n");
    body.push_str("event: content_block_start\ndata: ");
    body.push_str(
        &json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}})
            .to_string(),
    );
    body.push_str("\n\n");
    for chunk in chunks {
        body.push_str("event: content_block_delta\ndata: ");
        body.push_str(
            &json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": chunk}
            })
            .to_string(),
        );
        body.push_str("\n\n");
    }
    body
}
