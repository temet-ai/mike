use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ResponsesSummary {
    pub(crate) events: usize,
    pub(crate) response_id: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) state: Option<String>,
    pub(crate) usage: Option<Value>,
    pub(crate) output_items: Vec<Value>,
    pub(crate) error: Option<String>,
    pub(crate) undecodable_frames: usize,
}

pub(crate) fn summarize_stream(body: &[u8]) -> ResponsesSummary {
    let text = String::from_utf8_lossy(body).replace("\r\n", "\n");
    let mut summary = ResponsesSummary::default();
    for frame in text.split("\n\n") {
        let Some(data) = frame_data(frame) else {
            continue;
        };
        summary.events += 1;
        match serde_json::from_str::<Value>(&data) {
            Ok(event) => apply_event(&mut summary, &event),
            Err(_) => summary.undecodable_frames += 1,
        }
    }
    summary
}

pub(crate) fn summarize_json(body: &[u8]) -> ResponsesSummary {
    let mut summary = ResponsesSummary::default();
    if let Ok(response) = serde_json::from_slice::<Value>(body) {
        apply_response(&mut summary, &response);
        if let Some(items) = response.get("output").and_then(Value::as_array) {
            summary.output_items = items.clone();
        }
    }
    summary
}

fn frame_data(frame: &str) -> Option<String> {
    let lines: Vec<&str> = frame
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(|rest| rest.strip_prefix(' ').unwrap_or(rest))
        .collect();
    if lines.is_empty() {
        return None;
    }
    let data = lines.join("\n");
    (data != "[DONE]").then_some(data)
}

fn apply_event(summary: &mut ResponsesSummary, event: &Value) {
    let kind = event.get("type").and_then(Value::as_str).unwrap_or("");
    match kind {
        "response.output_item.done" => {
            if let Some(item) = event.get("item") {
                summary.output_items.push(item.clone());
            }
        }
        "error" => summary.error = Some(error_text(event)),
        _ => {}
    }
    if let Some(response) = event.get("response") {
        apply_response(summary, response);
    }
}

fn apply_response(summary: &mut ResponsesSummary, response: &Value) {
    let text = |key: &str| {
        response
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    if let Some(id) = text("id") {
        summary.response_id = Some(id);
    }
    if let Some(model) = text("model") {
        summary.model = Some(model);
    }
    if let Some(state) = text("status") {
        summary.state = Some(state);
    }
    if let Some(usage) = response.get("usage").filter(|u| !u.is_null()) {
        summary.usage = Some(usage.clone());
    }
    if let Some(error) = response.get("error").filter(|e| !e.is_null()) {
        summary.error = Some(error_text(error));
    }
}

fn error_text(error: &Value) -> String {
    error
        .get("message")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| error.to_string())
}

#[cfg(test)]
#[path = "responses_tests.rs"]
mod responses_tests;
