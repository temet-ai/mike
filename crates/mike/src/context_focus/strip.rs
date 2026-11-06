use std::collections::HashMap;

use bytes::Bytes;
use futures::StreamExt;
use serde_json::Value;

use super::FIELD;

pub(crate) fn strip_input(input: &mut Value) -> Option<Value> {
    input.as_object_mut()?.remove(FIELD)
}

pub(crate) fn strip_json_body(raw: &[u8]) -> Option<Bytes> {
    let mut value: Value = serde_json::from_slice(raw).ok()?;
    let content = value.get_mut("content")?.as_array_mut()?;
    let removed = content
        .iter_mut()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_use"))
        .filter_map(|block| block.get_mut("input").and_then(strip_input))
        .count();
    if removed == 0 {
        return None;
    }
    crate::filter::content_blocks::splice_top_level_keys(raw, &value, &["content"])
        .or_else(|| serde_json::to_vec(&value).ok())
        .map(Bytes::from)
}

#[derive(Default)]
pub(crate) struct SseStripper {
    pending: Vec<u8>,
    held: HashMap<u64, String>,
}

impl SseStripper {
    pub(crate) fn push(&mut self, chunk: &[u8]) -> Vec<u8> {
        self.pending.extend_from_slice(chunk);
        let mut out = Vec::new();
        while let Some(end) = find_event_end(&self.pending) {
            let event: Vec<u8> = self.pending.drain(..end).collect();
            self.relay(&event, &mut out);
        }
        out
    }

    pub(crate) fn finish(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.pending)
    }

    fn relay(&mut self, event: &[u8], out: &mut Vec<u8>) {
        let Some(data) = event_data(event) else {
            out.extend_from_slice(event);
            return;
        };
        let kind = data.get("type").and_then(Value::as_str).unwrap_or_default();
        let index = data.get("index").and_then(Value::as_u64);
        match (kind, index) {
            ("content_block_start", Some(index)) if starts_tool_use(&data) => {
                self.held.insert(index, String::new());
                out.extend_from_slice(event);
            }
            ("content_block_delta", Some(index)) if self.held.contains_key(&index) => {
                match data.pointer("/delta/partial_json").and_then(Value::as_str) {
                    Some(fragment) => self.held.entry(index).or_default().push_str(fragment),
                    None => out.extend_from_slice(event),
                }
            }
            ("content_block_stop", Some(index)) => {
                if let Some(json) = self.held.remove(&index) {
                    out.extend_from_slice(&cleaned_delta_event(index, &json));
                }
                out.extend_from_slice(event);
            }
            _ => out.extend_from_slice(event),
        }
    }
}

fn find_event_end(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\n\n").map(|p| p + 2)
}

fn event_data(event: &[u8]) -> Option<Value> {
    let text = std::str::from_utf8(event).ok()?;
    let data = text
        .lines()
        .find_map(|line| line.strip_prefix("data:"))?
        .trim_start();
    serde_json::from_str(data).ok()
}

fn starts_tool_use(data: &Value) -> bool {
    data.pointer("/content_block/type").and_then(Value::as_str) == Some("tool_use")
}

fn cleaned_delta_event(index: u64, json: &str) -> Vec<u8> {
    if json.is_empty() {
        return Vec::new();
    }
    let cleaned = match serde_json::from_str::<Value>(json) {
        Ok(mut input) => {
            strip_input(&mut input);
            serde_json::to_string(&input).unwrap_or_else(|_| json.to_owned())
        }
        Err(_) => json.to_owned(),
    };
    let data = serde_json::json!({
        "type": "content_block_delta",
        "index": index,
        "delta": { "type": "input_json_delta", "partial_json": cleaned },
    });
    format!("event: content_block_delta\ndata: {data}\n\n").into_bytes()
}

pub(crate) fn strip_stream<S, E>(
    upstream: S,
) -> impl futures::Stream<Item = Result<Bytes, std::io::Error>>
where
    S: futures::Stream<Item = Result<Bytes, E>> + Unpin,
    E: std::fmt::Display,
{
    futures::stream::unfold(
        Some((upstream, SseStripper::default())),
        |state| async move {
            let (mut upstream, mut stripper) = state?;
            loop {
                match upstream.next().await {
                    Some(Ok(chunk)) => {
                        let out = stripper.push(&chunk);
                        if !out.is_empty() {
                            return Some((Ok(Bytes::from(out)), Some((upstream, stripper))));
                        }
                    }
                    Some(Err(e)) => return Some((Err(std::io::Error::other(e.to_string())), None)),
                    None => {
                        let rest = stripper.finish();
                        return (!rest.is_empty()).then(|| (Ok(Bytes::from(rest)), None));
                    }
                }
            }
        },
    )
}
