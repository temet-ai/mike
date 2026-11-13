use bytes::Bytes;
use serde_json::{Map, Value};

use super::FIELD;
use super::strict;

pub const MAX_CHARS: u64 = 200;

const FILE_EDITING_TOOLS: [&str; 4] = ["Edit", "Write", "MultiEdit", "NotebookEdit"];

const PLAIN_DESCRIPTION: &str = "One short line saying what you want to find out or get done with this call, for example \"where is the retry limit set\" or \"does this test still fail\". At most 200 characters.";

const EMPHATIC_DESCRIPTION: &str = "Required on every call of this tool, Read included: one short line, written before the other arguments, saying what you want to find out or get done with this call, for example \"where is the retry limit set\" or \"does this test still fail\". At most 200 characters. The call is recorded with it.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Plain,
    Emphatic,
    Strict,
}

impl Mode {
    pub fn parse(setting: &str) -> Self {
        match setting {
            "emphatic" => Self::Emphatic,
            "strict" => Self::Strict,
            _ => Self::Plain,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Emphatic => "emphatic",
            Self::Strict => "strict",
        }
    }
}

pub fn field_schema(mode: Mode) -> Value {
    match mode {
        Mode::Plain => serde_json::json!({
            "type": "string",
            "maxLength": MAX_CHARS,
            "description": PLAIN_DESCRIPTION,
        }),
        Mode::Emphatic => serde_json::json!({
            "type": "string",
            "maxLength": MAX_CHARS,
            "description": EMPHATIC_DESCRIPTION,
        }),
        Mode::Strict => serde_json::json!({
            "type": "string",
            "description": EMPHATIC_DESCRIPTION,
        }),
    }
}

pub enum InjectOutcome {
    Unchanged,
    Injected {
        new_body: Bytes,
        tools: Vec<String>,
        strict_tools: Vec<String>,
    },
}

pub fn inject(raw: &[u8], mode: Mode) -> InjectOutcome {
    let Ok(mut value) = serde_json::from_slice::<Value>(raw) else {
        return InjectOutcome::Unchanged;
    };
    let Some(Value::Array(tools)) = value.get_mut("tools") else {
        return InjectOutcome::Unchanged;
    };
    let injected: Vec<String> = tools
        .iter_mut()
        .filter_map(|tool| add_field(tool, mode))
        .collect();
    if injected.is_empty() {
        return InjectOutcome::Unchanged;
    }
    let strict_tools = if mode == Mode::Strict {
        strict::make_strict(tools, &injected)
    } else {
        Vec::new()
    };
    let spliced = crate::filter::content_blocks::splice_top_level_keys(raw, &value, &["tools"])
        .or_else(|| serde_json::to_vec(&value).ok());
    match spliced {
        Some(bytes) => InjectOutcome::Injected {
            new_body: Bytes::from(bytes),
            tools: injected,
            strict_tools,
        },
        None => InjectOutcome::Unchanged,
    }
}

fn add_field(tool: &mut Value, mode: Mode) -> Option<String> {
    let name = tool.get("name").and_then(Value::as_str)?.to_owned();
    if !takes_the_field(&name) {
        return None;
    }
    let schema = tool
        .get_mut("input_schema")
        .and_then(Value::as_object_mut)?;
    if schema.get("type").and_then(Value::as_str) != Some("object") {
        return None;
    }
    let properties = schema
        .entry("properties")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()?;
    if properties.contains_key(FIELD) {
        return None;
    }
    properties.insert(FIELD.to_owned(), field_schema(mode));
    if let Value::Array(required) = schema
        .entry("required")
        .or_insert_with(|| Value::Array(Vec::new()))
    {
        required.push(Value::String(FIELD.to_owned()));
    }
    Some(name)
}

fn takes_the_field(name: &str) -> bool {
    !FILE_EDITING_TOOLS.contains(&name) && !crate::proxy_tools::is_proxy_tool(name)
}
