use serde::Deserialize;
use serde_json::Value;

pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResponsesSummary {
    pub events: usize,
    pub response_id: Option<String>,
    pub model: Option<String>,
    pub state: Option<String>,
    pub usage: Option<Value>,
    pub error: Option<String>,
    pub undecodable_frames: usize,
    pub oversized_frames: usize,
    pub terminal: bool,
}

impl ResponsesSummary {
    pub fn model_call_state(&self, http_status: Option<u16>) -> Option<&str> {
        if http_status.is_some_and(|status| status >= 400) || self.error.is_some() {
            Some("failed")
        } else if let Some(state) = self.state.as_deref() {
            Some(state)
        } else {
            None
        }
    }
}

#[derive(Default, Deserialize)]
struct ResponseMetadata {
    id: Option<String>,
    model: Option<String>,
    status: Option<String>,
    usage: Option<Value>,
    error: Option<Value>,
}

#[derive(Deserialize)]
struct Event {
    #[serde(rename = "type", default)]
    kind: String,
    response: Option<ResponseMetadata>,
    message: Option<String>,
}

pub struct ResponsesStream {
    summary: ResponsesSummary,
    frame: Vec<u8>,
    delimiter: [u8; 4],
    oversized: bool,
    limit: usize,
}

impl Default for ResponsesStream {
    fn default() -> Self {
        Self::new(MAX_FRAME_BYTES)
    }
}

impl ResponsesStream {
    pub fn new(max_frame_bytes: usize) -> Self {
        Self {
            summary: ResponsesSummary::default(),
            frame: Vec::new(),
            delimiter: [0; 4],
            oversized: false,
            limit: max_frame_bytes.min(MAX_FRAME_BYTES),
        }
    }

    pub fn push(&mut self, chunk: &[u8]) {
        for &byte in chunk {
            self.delimiter.rotate_left(1);
            self.delimiter[3] = byte;
            if !self.oversized {
                if self.frame.len() == self.limit {
                    self.frame.clear();
                    self.oversized = true;
                    self.summary.oversized_frames += 1;
                } else {
                    self.frame.push(byte);
                }
            }
            if self.delimiter.ends_with(b"\n\n") || self.delimiter == *b"\r\n\r\n" {
                self.finish_frame();
            }
        }
    }

    pub fn summary(&self) -> &ResponsesSummary {
        &self.summary
    }

    pub fn finish(mut self) -> ResponsesSummary {
        self.finish_frame();
        self.summary
    }

    fn finish_frame(&mut self) {
        if !self.oversized && !self.frame.is_empty() {
            apply_frame(&mut self.summary, &self.frame);
        }
        self.frame.clear();
        self.delimiter = [0; 4];
        self.oversized = false;
    }
}

fn apply_frame(summary: &mut ResponsesSummary, bytes: &[u8]) {
    let Ok(frame) = std::str::from_utf8(bytes) else {
        summary.undecodable_frames += 1;
        return;
    };
    let lines: Vec<&str> = frame
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(|line| line.strip_prefix(' ').unwrap_or(line))
        .collect();
    if lines.is_empty() {
        return;
    }
    let data = lines.join("\n");
    if data == "[DONE]" {
        return;
    }
    summary.events += 1;
    let Ok(event) = serde_json::from_str::<Event>(&data) else {
        summary.undecodable_frames += 1;
        return;
    };
    if matches!(
        event.kind.as_str(),
        "response.completed" | "response.failed" | "response.incomplete" | "response.cancelled"
    ) {
        summary.terminal = true;
    }
    if event.kind == "error" {
        summary.error = Some(
            event
                .message
                .unwrap_or_else(|| "provider stream error".into()),
        );
    }
    if let Some(response) = event.response {
        apply_response(summary, response);
    }
}

fn apply_response(summary: &mut ResponsesSummary, response: ResponseMetadata) {
    if response.id.is_some() {
        summary.response_id = response.id;
    }
    if response.model.is_some() {
        summary.model = response.model;
    }
    if response.status.is_some() {
        summary.state = response.status;
    }
    if response
        .usage
        .as_ref()
        .is_some_and(|usage| !usage.is_null())
    {
        summary.usage = response.usage;
    }
    if let Some(error) = response.error.filter(|error| !error.is_null()) {
        summary.error = Some(
            error
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| error.to_string()),
        );
    }
}

pub fn summarize_stream(body: &[u8]) -> ResponsesSummary {
    let mut stream = ResponsesStream::default();
    stream.push(body);
    stream.finish()
}

pub fn summarize_json(body: &[u8]) -> ResponsesSummary {
    let mut summary = ResponsesSummary::default();
    if body.len() > MAX_FRAME_BYTES {
        summary.oversized_frames = 1;
    } else if let Ok(response) = serde_json::from_slice::<ResponseMetadata>(body) {
        summary.terminal = matches!(
            response.status.as_deref(),
            Some("completed" | "failed" | "incomplete" | "cancelled")
        );
        apply_response(&mut summary, response);
    } else {
        summary.undecodable_frames = 1;
    }
    summary
}

#[cfg(test)]
#[path = "responses_tests.rs"]
mod tests;
