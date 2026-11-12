use serde_json::json;

use super::{ResponsesStream, summarize_json, summarize_stream};

const COMPLETED: &[u8] = b"data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"observed\",\"status\":\"completed\",\"usage\":{\"input_tokens\":7,\"output_tokens\":3}}}\r\n\r\n";

#[test]
fn received_http_body_does_not_imply_model_call_success() {
    let empty = super::ResponsesSummary::default();
    assert_eq!(empty.model_call_state(Some(200)), None);
    for status in [400, 429, 500] {
        assert_eq!(empty.model_call_state(Some(status)), Some("failed"));
    }
    let error = summarize_stream(b"data: {\"type\":\"error\",\"message\":\"overloaded\"}\n\n");
    assert_eq!(error.model_call_state(Some(200)), Some("failed"));
    let error_after_start = summarize_stream(
        concat!(
            "data: {\"type\":\"response.created\",\"response\":{\"status\":\"in_progress\"}}\n\n",
            "data: {\"type\":\"error\"}\n\n"
        )
        .as_bytes(),
    );
    assert_eq!(
        error_after_start.model_call_state(Some(200)),
        Some("failed")
    );
    assert_eq!(
        summarize_stream(COMPLETED).model_call_state(Some(200)),
        Some("completed")
    );
}

#[test]
fn terminal_event_is_observed_before_transport_eof_for_every_chunk_boundary() {
    for boundary in 0..COMPLETED.len() {
        let mut stream = ResponsesStream::default();
        stream.push(&COMPLETED[..boundary]);
        assert!(!stream.summary().terminal);
        stream.push(&COMPLETED[boundary..]);
        assert!(stream.summary().terminal);
        assert_eq!(stream.summary().state.as_deref(), Some("completed"));
        assert_eq!(
            stream.summary().usage,
            Some(json!({"input_tokens":7,"output_tokens":3}))
        );
    }
}

#[test]
fn metadata_does_not_duplicate_output_items_or_delta_text() {
    let mut stream = ResponsesStream::default();
    stream.push(b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"private text\"}\n\n");
    stream.push(COMPLETED);
    let summary = stream.finish();
    assert_eq!(summary.events, 2);
    assert!(!format!("{summary:?}").contains("private text"));
}

#[test]
fn oversized_frame_is_discarded_whole_and_next_terminal_frame_recovers() {
    let mut stream = ResponsesStream::new(256);
    for _ in 0..10_000 {
        stream.push(b"x");
        assert!(stream.frame.len() <= 256);
    }
    stream.push(b"\r\n\r\n");
    stream.push(COMPLETED);
    let summary = stream.finish();
    assert_eq!(summary.oversized_frames, 1);
    assert!(summary.terminal);
    assert_eq!(summary.response_id.as_deref(), Some("resp_1"));
}

#[test]
fn failed_and_malformed_frames_preserve_known_state_and_report_gaps() {
    let summary = summarize_stream(concat!(
        "data: not json\n\n",
        "data: {\"type\":\"response.failed\",\"response\":{\"status\":\"failed\",\"error\":{\"message\":\"overloaded\"}}}\n\n",
        "data: [DONE]\n\n"
    ).as_bytes());
    assert!(summary.terminal);
    assert_eq!(summary.error.as_deref(), Some("overloaded"));
    assert_eq!(summary.undecodable_frames, 1);
    assert_eq!(summary.usage, None);
}

#[test]
fn json_metadata_preserves_unknown_usage_and_ignores_output() {
    let summary = summarize_json(
        br#"{"id":"r","status":"completed","usage":null,"output":[{"text":"payload"}]}"#,
    );
    assert!(summary.terminal);
    assert_eq!(summary.response_id.as_deref(), Some("r"));
    assert_eq!(summary.usage, None);
    assert!(!format!("{summary:?}").contains("payload"));
}

#[test]
fn multibyte_and_multiline_frames_survive_single_byte_chunks() {
    let mut stream = ResponsesStream::default();
    for byte in "data: {\"type\":\"response.created\",\n data is ignored\ndata: \"response\":{\"id\":\"héllo\"}}\n\n".as_bytes() {
        stream.push(&[*byte]);
    }
    assert_eq!(stream.finish().response_id.as_deref(), Some("héllo"));
}
