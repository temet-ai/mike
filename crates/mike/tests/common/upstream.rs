#![allow(dead_code)]

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub async fn mount_canned_json(server: &MockServer, msg_id: &str) {
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "id": msg_id,
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "text", "text": "ok"}],
                    "model": "claude-sonnet-4-20250514",
                    "stop_reason": "end_turn",
                    "usage": {"input_tokens": 1, "output_tokens": 1}
                }))
                .insert_header("content-type", "application/json"),
        )
        .mount(server)
        .await;
}

pub async fn canned_json_upstream(msg_id: &str) -> MockServer {
    let server = MockServer::start().await;
    mount_canned_json(&server, msg_id).await;
    server
}

pub async fn mount_sse(server: &MockServer, raw_sse: &str) {
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(raw_sse.as_bytes(), "text/event-stream"),
        )
        .mount(server)
        .await;
}

pub async fn sse_upstream(raw_sse: &str) -> MockServer {
    let server = MockServer::start().await;
    mount_sse(&server, raw_sse).await;
    server
}
