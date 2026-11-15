use std::collections::VecDeque;

use bytes::Bytes;
use serde_json::Value;

use super::frames::Frame;
use super::scanner::{DeltaScanner, Hit};
use super::stitch::{self, StitchPoint};
use crate::extract::response::{
    extract_input_json_delta, extract_text_delta, extract_tool_use_start,
};
use crate::proxy_tools::answer::{AssistantPrelude, ProxyToolCall};
use crate::proxy_tools::scanner::ToolCallScanner;

pub enum Cut {
    Word {
        word: String,
        text: String,
        kept: String,
        kept_frame: Option<Bytes>,
    },
    ToolCall {
        call: Box<ProxyToolCall>,
        prelude: Box<AssistantPrelude>,
    },
}

pub enum Admit {
    Forward(Vec<Bytes>),
    Cut(Cut),
}

struct Held {
    raw: Bytes,
    chars: usize,
    bytes: usize,
}

pub struct StreamGate {
    words: DeltaScanner,
    tools: Option<ToolCallScanner>,
    held_text: VecDeque<Held>,
    held_chars: usize,
    held_bytes: usize,
    open_block: Option<u64>,
    forwarded_blocks: u64,
    held_tool_block: Option<u64>,
}

impl StreamGate {
    pub fn new(forbidden_words: &[String], answers_tool_calls: bool) -> Self {
        Self {
            words: DeltaScanner::new(forbidden_words),
            tools: answers_tool_calls.then(ToolCallScanner::new),
            held_text: VecDeque::new(),
            held_chars: 0,
            held_bytes: 0,
            open_block: None,
            forwarded_blocks: 0,
            held_tool_block: None,
        }
    }

    pub fn admit(&mut self, frame: Frame) -> Admit {
        match frame.event.as_str() {
            "message_start" => self.on_message_start(frame),
            "content_block_start" => self.on_block_start(frame),
            "content_block_delta" => self.on_block_delta(frame),
            "content_block_stop" => self.on_block_stop(frame),
            _ => self.pass(frame.raw),
        }
    }

    pub fn finish(&mut self) -> Vec<Bytes> {
        self.release_all()
    }

    pub fn stitch_point(&self) -> StitchPoint {
        StitchPoint {
            open_block: self.open_block,
            next_index: self.forwarded_blocks,
        }
    }

    fn on_message_start(&mut self, frame: Frame) -> Admit {
        self.words.reset();
        if let Some(tools) = self.tools.as_mut() {
            tools.reset();
        }
        self.pass(frame.raw)
    }

    fn on_block_start(&mut self, frame: Frame) -> Admit {
        let json = parse(&frame.data);
        let index = json.as_ref().and_then(index_of);
        if let Some(json) = json.as_ref()
            && self.opens_a_proxy_tool_call(json)
        {
            self.held_tool_block = Some(index.unwrap_or(self.forwarded_blocks));
            return Admit::Forward(self.release_all());
        }
        if let Some(index) = index {
            self.open_block = Some(index);
            self.forwarded_blocks = self.forwarded_blocks.max(index + 1);
        }
        self.pass(frame.raw)
    }

    fn opens_a_proxy_tool_call(&mut self, json: &Value) -> bool {
        let Some(tools) = self.tools.as_mut() else {
            return false;
        };
        if json.pointer("/content_block/type").and_then(Value::as_str) != Some("tool_use") {
            return false;
        }
        let Some((id, name)) = extract_tool_use_start(json) else {
            return false;
        };
        if !crate::proxy_tools::is_proxy_tool(&name) {
            return false;
        }
        tools.open_tool_use(&id, &name);
        true
    }

    fn on_block_delta(&mut self, frame: Frame) -> Admit {
        let Some(json) = parse(&frame.data) else {
            return self.pass(frame.raw);
        };
        if self.held_tool_block.is_some() && self.held_tool_block == index_of(&json) {
            if let Some(fragment) = extract_input_json_delta(&json)
                && let Some(tools) = self.tools.as_mut()
            {
                tools.push_input_json(&fragment);
            }
            return Admit::Forward(Vec::new());
        }
        if json.pointer("/delta/type").and_then(Value::as_str) != Some("text_delta") {
            return self.pass(frame.raw);
        }
        let Some(text) = extract_text_delta(&json) else {
            return self.pass(frame.raw);
        };
        if let Some(tools) = self.tools.as_mut() {
            tools.push_text(&text);
        }
        if let Some(hit) = self.words.push(&text) {
            return Admit::Cut(self.cut_at_word(hit, text.len()));
        }
        self.hold_text(Held {
            raw: frame.raw,
            chars: text.chars().count(),
            bytes: text.len(),
        })
    }

    fn cut_at_word(&mut self, hit: Hit, chunk_bytes: usize) -> Cut {
        let text = self.words.text().to_owned();
        let unsent = text.len().saturating_sub(self.held_bytes + chunk_bytes);
        self.release_all();
        let kept = text.get(..hit.start).unwrap_or_default().to_owned();
        let kept_frame = match (self.open_block, kept.get(unsent..)) {
            (Some(index), Some(tail)) if !tail.is_empty() => {
                Some(stitch::text_delta_frame(index, tail))
            }
            _ => None,
        };
        Cut::Word {
            word: hit.word,
            text,
            kept,
            kept_frame,
        }
    }

    fn on_block_stop(&mut self, frame: Frame) -> Admit {
        let index = parse(&frame.data).as_ref().and_then(index_of);
        if self.held_tool_block.is_some()
            && self.held_tool_block == index
            && let Some((call, prelude)) =
                self.tools.as_mut().and_then(ToolCallScanner::close_block)
        {
            return Admit::Cut(Cut::ToolCall {
                call: Box::new(call),
                prelude: Box::new(prelude),
            });
        }
        if self.open_block == index {
            self.open_block = None;
        }
        self.pass(frame.raw)
    }

    fn hold_text(&mut self, held: Held) -> Admit {
        self.held_chars += held.chars;
        self.held_bytes += held.bytes;
        self.held_text.push_back(held);
        let keep = self.words.hold_chars();
        let mut released = Vec::new();
        while let Some(front) = self.held_text.front() {
            if self.held_chars - front.chars < keep {
                break;
            }
            if let Some(front) = self.held_text.pop_front() {
                self.held_chars -= front.chars;
                self.held_bytes -= front.bytes;
                released.push(front.raw);
            }
        }
        Admit::Forward(released)
    }

    fn pass(&mut self, raw: Bytes) -> Admit {
        let mut frames = self.release_all();
        frames.push(raw);
        Admit::Forward(frames)
    }

    fn release_all(&mut self) -> Vec<Bytes> {
        self.held_chars = 0;
        self.held_bytes = 0;
        self.held_text.drain(..).map(|held| held.raw).collect()
    }
}

fn parse(data: &str) -> Option<Value> {
    serde_json::from_str(data).ok()
}

fn index_of(json: &Value) -> Option<u64> {
    json.get("index").and_then(Value::as_u64)
}

#[cfg(test)]
#[path = "gate_tests.rs"]
mod tests;
