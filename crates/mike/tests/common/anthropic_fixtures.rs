#![allow(dead_code)]

use serde_json::{Value, json};

pub const DEFAULT_MODEL: &str = "claude-sonnet-4-20250514";

pub fn text_block(text: impl Into<Value>) -> Value {
    json!({"type": "text", "text": text.into()})
}

pub fn cached_text_block(text: impl Into<Value>) -> Value {
    json!({"type": "text", "text": text.into(), "cache_control": {"type": "ephemeral"}})
}

pub fn tool_use_block(id: &str, name: &str, input: Value) -> Value {
    json!({"type": "tool_use", "id": id, "name": name, "input": input})
}

pub fn tool_result_block(tool_use_id: &str, content: impl Into<Value>) -> Value {
    json!({"type": "tool_result", "tool_use_id": tool_use_id, "content": content.into()})
}

pub fn user_message(content: impl Into<Value>) -> Value {
    json!({"role": "user", "content": content.into()})
}

pub fn assistant_message(content: impl Into<Value>) -> Value {
    json!({"role": "assistant", "content": content.into()})
}

pub struct RequestBody {
    model: String,
    max_tokens: u32,
    stream: Option<bool>,
    system: Option<Value>,
    messages: Value,
}

impl Default for RequestBody {
    fn default() -> Self {
        Self {
            model: DEFAULT_MODEL.to_string(),
            max_tokens: 1024,
            stream: None,
            system: None,
            messages: json!([user_message("Hello")]),
        }
    }
}

impl RequestBody {
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    pub fn max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    pub fn stream(mut self, stream: bool) -> Self {
        self.stream = Some(stream);
        self
    }

    pub fn system(mut self, system: impl Into<Value>) -> Self {
        self.system = Some(system.into());
        self
    }

    pub fn messages(mut self, messages: impl Into<Value>) -> Self {
        self.messages = messages.into();
        self
    }

    pub fn content(self, content: impl Into<Value>) -> Self {
        self.messages(json!([user_message(content)]))
    }

    pub fn build(self) -> Value {
        let mut body = json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
            "messages": self.messages,
        });
        if let Some(stream) = self.stream {
            body["stream"] = json!(stream);
        }
        if let Some(system) = self.system {
            body["system"] = system;
        }
        body
    }
}

pub struct MockResponse {
    id: String,
    minimal: bool,
    typed: bool,
    text: String,
    input_tokens: u32,
    output_tokens: u32,
}

impl MockResponse {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            minimal: false,
            typed: false,
            text: "ok".to_string(),
            input_tokens: 10,
            output_tokens: 5,
        }
    }

    pub fn minimal(mut self) -> Self {
        self.minimal = true;
        self
    }

    pub fn typed(mut self) -> Self {
        self.typed = true;
        self
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }

    pub fn usage(mut self, input_tokens: u32, output_tokens: u32) -> Self {
        self.input_tokens = input_tokens;
        self.output_tokens = output_tokens;
        self
    }

    pub fn build(self) -> Value {
        if self.minimal {
            let mut body = json!({"id": self.id, "stop_reason": "end_turn"});
            if self.typed {
                body["type"] = json!("message");
            }
            return body;
        }
        json!({
            "id": self.id,
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": self.text}],
            "model": DEFAULT_MODEL,
            "stop_reason": "end_turn",
            "usage": {"input_tokens": self.input_tokens, "output_tokens": self.output_tokens}
        })
    }
}
