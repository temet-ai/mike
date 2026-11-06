use serde_json::json;
use sse::sse_body;

use crate::sse;

pub(super) type SseSequence = (String, String, Vec<(String, String)>);

pub(super) fn realistic_sse_sequence() -> anyhow::Result<SseSequence> {
    let events: Vec<(&str, String)> = vec![
        (
            "message_start",
            serde_json::to_string(&json!({
                "type": "message_start",
                "message": {
                    "id": "msg_e2e_smoke_001",
                    "type": "message",
                    "role": "assistant",
                    "content": [],
                    "model": "claude-sonnet-4-20250514",
                    "stop_reason": null,
                    "stop_sequence": null,
                    "usage": {
                        "input_tokens": 25,
                        "output_tokens": 0,
                        "cache_creation_input_tokens": 0,
                        "cache_read_input_tokens": 0
                    }
                }
            }))?,
        ),
        (
            "content_block_start",
            serde_json::to_string(&json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": {"type": "text", "text": ""}
            }))?,
        ),
        ("ping", r#"{"type":"ping"}"#.to_string()),
        (
            "content_block_delta",
            serde_json::to_string(&json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": "Hello"}
            }))?,
        ),
        (
            "content_block_delta",
            serde_json::to_string(&json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": ", "}
            }))?,
        ),
        (
            "content_block_delta",
            serde_json::to_string(&json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": "world!"}
            }))?,
        ),
        (
            "content_block_stop",
            serde_json::to_string(&json!({
                "type": "content_block_stop",
                "index": 0
            }))?,
        ),
        (
            "message_delta",
            serde_json::to_string(&json!({
                "type": "message_delta",
                "delta": {"stop_reason": "end_turn", "stop_sequence": null},
                "usage": {"output_tokens": 12}
            }))?,
        ),
        (
            "message_stop",
            serde_json::to_string(&json!({
                "type": "message_stop"
            }))?,
        ),
    ];

    let event_refs: Vec<(&str, &str)> = events.iter().map(|(e, d)| (*e, d.as_str())).collect();
    let raw_sse = sse_body(&event_refs);

    let owned_events: Vec<(String, String)> = events
        .into_iter()
        .map(|(e, d)| (e.to_string(), d))
        .collect();

    Ok((raw_sse, "Hello, world!".to_string(), owned_events))
}

pub(super) fn rate_limit_headers() -> Vec<(&'static str, &'static str)> {
    vec![
        ("anthropic-ratelimit-requests-limit", "1000"),
        ("anthropic-ratelimit-requests-remaining", "999"),
        ("anthropic-ratelimit-requests-reset", "2025-01-01T00:00:00Z"),
        ("anthropic-ratelimit-tokens-limit", "100000"),
        ("anthropic-ratelimit-tokens-remaining", "99000"),
        ("anthropic-ratelimit-tokens-reset", "2025-01-01T00:00:00Z"),
        ("anthropic-ratelimit-input-tokens-limit", "80000"),
        ("anthropic-ratelimit-input-tokens-remaining", "79000"),
        (
            "anthropic-ratelimit-input-tokens-reset",
            "2025-01-01T00:00:10Z",
        ),
        ("anthropic-ratelimit-output-tokens-limit", "20000"),
        ("anthropic-ratelimit-output-tokens-remaining", "19500"),
        (
            "anthropic-ratelimit-output-tokens-reset",
            "2025-01-01T00:00:20Z",
        ),
        ("retry-after", "42"),
        ("request-id", "req-e2e-smoke-001"),
    ]
}

pub(super) fn assert_rate_limit_headers_forwarded(headers: &reqwest::header::HeaderMap) {
    for (name, expected_value) in rate_limit_headers() {
        let actual = headers
            .get(name)
            .unwrap_or_else(|| panic!("missing rate-limit header: {name}"))
            .to_str()
            .unwrap_or_else(|_| panic!("non-string header value for: {name}"));

        assert_eq!(
            actual, expected_value,
            "header {name}: expected '{expected_value}', got '{actual}'"
        );
    }
}
