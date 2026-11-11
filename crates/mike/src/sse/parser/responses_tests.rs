use serde_json::json;

use super::{summarize_json, summarize_stream};

const STREAM: &str = concat!(
    "event: response.created\n",
    "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"status\":\"in_progress\",\"model\":\"gpt-x\"}}\n\n",
    "event: response.output_text.delta\r\n",
    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\r\n\r\n",
    "event: response.output_item.done\n",
    "data: {\"type\":\"response.output_item.done\",\"item\":{\"id\":\"msg_1\",\"type\":\"message\"}}\n\n",
    "event: response.completed\n",
    "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"status\":\"completed\",\"model\":\"gpt-x-2\",\"usage\":{\"input_tokens\":10,\"output_tokens\":2}}}\n\n",
);

#[test]
fn summarize_stream_completed_reads_id_model_usage_and_items() {
    let summary = summarize_stream(STREAM.as_bytes());
    assert_eq!(summary.events, 4);
    assert_eq!(summary.response_id.as_deref(), Some("resp_1"));
    assert_eq!(summary.model.as_deref(), Some("gpt-x-2"));
    assert_eq!(summary.state.as_deref(), Some("completed"));
    assert_eq!(
        summary.usage,
        Some(json!({"input_tokens": 10, "output_tokens": 2}))
    );
    assert_eq!(
        summary.output_items,
        vec![json!({"id": "msg_1", "type": "message"})]
    );
    assert_eq!(summary.error, None);
}

#[test]
fn summarize_stream_failed_and_error_events_keep_the_message() {
    let body = concat!(
        "data: {\"type\":\"response.failed\",\"response\":{\"id\":\"r\",\"status\":\"failed\",\"error\":{\"message\":\"overloaded\"}}}\n\n",
        "data: not json\n\n",
        "data: [DONE]\n\n",
    );
    let summary = summarize_stream(body.as_bytes());
    assert_eq!(summary.state.as_deref(), Some("failed"));
    assert_eq!(summary.error.as_deref(), Some("overloaded"));
    assert_eq!(summary.undecodable_frames, 1);
    assert_eq!(summary.events, 2);
}

#[test]
fn summarize_stream_missing_usage_stays_unknown() {
    let body =
        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"r\",\"usage\":null}}\n\n";
    let summary = summarize_stream(body.as_bytes());
    assert_eq!(summary.usage, None);
}

#[test]
fn summarize_json_reads_a_buffered_response() {
    let body = json!({"id": "resp_2", "status": "completed", "model": "m", "output": [{"type": "message"}], "usage": {"total_tokens": 3}});
    let summary = summarize_json(body.to_string().as_bytes());
    assert_eq!(summary.response_id.as_deref(), Some("resp_2"));
    assert_eq!(summary.output_items.len(), 1);
    assert_eq!(summary.usage, Some(json!({"total_tokens": 3})));
}
