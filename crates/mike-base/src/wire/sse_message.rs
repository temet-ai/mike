use std::collections::BTreeMap;

use serde_json::{Map, Value};

pub fn assistant_message_from_sse(sse: &[u8]) -> Option<Value> {
    let text = String::from_utf8_lossy(sse);
    let mut assembler = Assembler::default();
    for line in text.lines() {
        if let Some(event) = parse_data_line(line) {
            assembler.apply(&event);
        }
    }
    assembler.finish()
}

#[derive(Default)]
pub struct SseReplyAssembler(Assembler);

impl SseReplyAssembler {
    pub fn apply_data(&mut self, data: &str) {
        if let Ok(event) = serde_json::from_str::<Value>(data) {
            self.0.apply(&event);
        }
    }

    pub fn finish(self) -> Option<Value> {
        self.0.finish()
    }
}

fn parse_data_line(line: &str) -> Option<Value> {
    let payload = line.strip_prefix("data:")?;
    let payload = payload.strip_prefix(' ').unwrap_or(payload);
    serde_json::from_str(payload).ok()
}

#[derive(Default)]
struct Assembler {
    message: Option<Map<String, Value>>,
    blocks: BTreeMap<usize, Value>,
    json_buffers: BTreeMap<usize, String>,
}

impl Assembler {
    fn apply(&mut self, event: &Value) {
        match event.get("type").and_then(Value::as_str) {
            Some("message_start") => self.message_start(event),
            Some("content_block_start") => self.block_start(event),
            Some("content_block_delta") => self.block_delta(event),
            Some("content_block_stop") => self.block_stop(event),
            Some("message_delta") => self.message_delta(event),
            _ => {}
        }
    }

    fn message_start(&mut self, event: &Value) {
        if let Some(message) = event.get("message").and_then(Value::as_object) {
            self.message = Some(message.clone());
            self.blocks.clear();
            self.json_buffers.clear();
        }
    }

    fn block_start(&mut self, event: &Value) {
        let (Some(index), Some(block)) = (block_index(event), event.get("content_block")) else {
            return;
        };
        self.blocks.insert(index, block.clone());
        self.json_buffers.remove(&index);
    }

    fn block_delta(&mut self, event: &Value) {
        let (Some(index), Some(delta)) = (block_index(event), event.get("delta")) else {
            return;
        };
        let Some(kind) = delta.get("type").and_then(Value::as_str) else {
            return;
        };
        if kind == "input_json_delta" {
            self.append_partial_json(index, delta);
            return;
        }
        if let Some(block) = self.blocks.get_mut(&index) {
            apply_block_delta(block, kind, delta);
        }
    }

    fn append_partial_json(&mut self, index: usize, delta: &Value) {
        if !self.blocks.contains_key(&index) {
            return;
        }
        if let Some(fragment) = delta.get("partial_json").and_then(Value::as_str) {
            self.json_buffers
                .entry(index)
                .or_default()
                .push_str(fragment);
        }
    }

    fn block_stop(&mut self, event: &Value) {
        let Some(index) = block_index(event) else {
            return;
        };
        if let Some(buffer) = self.json_buffers.remove(&index) {
            self.apply_input(index, &buffer);
        }
    }

    fn apply_input(&mut self, index: usize, buffer: &str) {
        let Some(block) = self.blocks.get_mut(&index) else {
            return;
        };
        let is_tool_use = block
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|kind| kind.ends_with("tool_use"));
        if !is_tool_use || buffer.trim().is_empty() {
            return;
        }
        if let (Some(object), Ok(input)) =
            (block.as_object_mut(), serde_json::from_str::<Value>(buffer))
        {
            object.insert("input".to_string(), input);
        }
    }

    fn message_delta(&mut self, event: &Value) {
        let Some(message) = self.message.as_mut() else {
            return;
        };
        if let Some(delta) = event.get("delta").and_then(Value::as_object) {
            copy_stop_fields(message, delta);
        }
        if let Some(usage) = event.get("usage").and_then(Value::as_object) {
            merge_usage(message, usage);
        }
    }

    fn finish(mut self) -> Option<Value> {
        let pending = std::mem::take(&mut self.json_buffers);
        for (index, buffer) in pending {
            self.apply_input(index, &buffer);
        }
        let mut message = self.message?;
        let content = self.blocks.into_values().collect();
        message.insert("content".to_string(), Value::Array(content));
        Some(Value::Object(message))
    }
}

fn block_index(event: &Value) -> Option<usize> {
    let index = event.get("index")?.as_u64()?;
    usize::try_from(index).ok()
}

fn apply_block_delta(block: &mut Value, kind: &str, delta: &Value) {
    match kind {
        "text_delta" => append_string(block, "text", delta.get("text")),
        "thinking_delta" => append_string(block, "thinking", delta.get("thinking")),
        "signature_delta" => set_field(block, "signature", delta.get("signature")),
        "citations_delta" => push_citation(block, delta.get("citation")),
        _ => {}
    }
}

fn append_string(block: &mut Value, key: &str, addition: Option<&Value>) {
    let (Some(object), Some(addition)) = (block.as_object_mut(), addition.and_then(Value::as_str))
    else {
        return;
    };
    let entry = object
        .entry(key)
        .or_insert_with(|| Value::String(String::new()));
    if !entry.is_string() {
        *entry = Value::String(String::new());
    }
    if let Value::String(current) = entry {
        current.push_str(addition);
    }
}

fn set_field(block: &mut Value, key: &str, value: Option<&Value>) {
    if let (Some(object), Some(value)) = (block.as_object_mut(), value) {
        object.insert(key.to_string(), value.clone());
    }
}

fn push_citation(block: &mut Value, citation: Option<&Value>) {
    let (Some(object), Some(citation)) = (block.as_object_mut(), citation) else {
        return;
    };
    let entry = object
        .entry("citations")
        .or_insert_with(|| Value::Array(Vec::new()));
    if !entry.is_array() {
        *entry = Value::Array(Vec::new());
    }
    if let Some(items) = entry.as_array_mut() {
        items.push(citation.clone());
    }
}

fn copy_stop_fields(message: &mut Map<String, Value>, delta: &Map<String, Value>) {
    for key in ["stop_reason", "stop_sequence"] {
        if let Some(value) = delta.get(key) {
            message.insert(key.to_string(), value.clone());
        }
    }
}

fn merge_usage(message: &mut Map<String, Value>, usage: &Map<String, Value>) {
    let slot = message
        .entry("usage")
        .or_insert_with(|| Value::Object(Map::new()));
    if !slot.is_object() {
        *slot = Value::Object(Map::new());
    }
    let Some(target) = slot.as_object_mut() else {
        return;
    };
    for (key, value) in usage.iter().filter(|(_, value)| !value.is_null()) {
        target.insert(key.clone(), value.clone());
    }
}

#[cfg(test)]
#[path = "sse_message_tests.rs"]
mod tests;
