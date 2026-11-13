use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::observation::CallFact;
use super::tool_effect::{ToolEffect, classify};

static REDIRECT: LazyLock<Regex> = LazyLock::new(|| compile(r">>?\s*([^\s&|;>]+)"));
static TEE: LazyLock<Regex> = LazyLock::new(|| compile(r"\btee\s+(?:-\w+\s+)*([^\s&|;>]+)"));
static READ_VERB: LazyLock<Regex> = LazyLock::new(|| {
    compile(r"(^|[;&|]\s*)(cat|head|tail|less|more|bat|grep|rg|diff|wc|jq|stat|sed\s+-n)\s")
});

fn compile(pattern: &str) -> Regex {
    match Regex::new(pattern) {
        Ok(re) => re,
        Err(e) => panic!("invalid read-back pattern {pattern:?}: {e}"),
    }
}

fn clean(token: &str) -> String {
    token.trim_matches(['"', '\'', '`']).to_owned()
}

fn input_path(input: &Value, key: &str) -> Option<String> {
    input
        .get(key)
        .and_then(Value::as_str)
        .filter(|p| !p.is_empty())
        .map(clean)
}

fn command_of(input: &Value) -> &str {
    input
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn bash_written(command: &str) -> Vec<String> {
    REDIRECT
        .captures_iter(command)
        .chain(TEE.captures_iter(command))
        .filter_map(|c| c.get(1))
        .map(|m| clean(m.as_str()))
        .filter(|p| p != "/dev/null" && !p.starts_with('&'))
        .collect()
}

pub fn written_paths(call: &CallFact) -> Vec<String> {
    match call.tool_name.as_str() {
        "Edit" | "Write" | "MultiEdit" => {
            input_path(&call.input, "file_path").into_iter().collect()
        }
        "NotebookEdit" => input_path(&call.input, "notebook_path")
            .into_iter()
            .collect(),
        "Bash" => bash_written(command_of(&call.input)),
        _ => Vec::new(),
    }
}

fn bash_read(command: &str) -> Vec<String> {
    if !READ_VERB.is_match(command) {
        return Vec::new();
    }
    command
        .split(|c: char| c.is_whitespace() || matches!(c, ';' | '|' | '&'))
        .filter(|t| !t.is_empty() && !t.starts_with('-'))
        .map(clean)
        .collect()
}

fn read_paths(call: &CallFact) -> Vec<String> {
    match call.tool_name.as_str() {
        "Read" => input_path(&call.input, "file_path").into_iter().collect(),
        "Bash" => bash_read(command_of(&call.input)),
        _ => Vec::new(),
    }
}

fn same_file(read: &str, written: &str) -> bool {
    let read = read.trim_start_matches("./");
    let written = written.trim_start_matches("./");
    read == written
        || written.ends_with(&format!("/{read}"))
        || read.ends_with(&format!("/{written}"))
}

pub fn reads_back(call: &CallFact, written: &[String]) -> bool {
    if !call.has_result
        || matches!(
            classify(&call.tool_name, &call.input),
            ToolEffect::Mutating(_)
        )
    {
        return false;
    }
    let read = read_paths(call);
    written.iter().any(|w| read.iter().any(|r| same_file(r, w)))
}
