use std::collections::{HashMap, HashSet};

use serde_json::Value;

pub const SUPERSEDED_MARKER_TAG: &str = "[[mike:file_read_superseded]]";

struct ReadCall<'a> {
    id: &'a str,
    path: &'a str,
    whole: bool,
    msg_idx: usize,
}

fn blocks(message: &Value) -> impl Iterator<Item = &Value> {
    message
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

fn block_type(block: &Value) -> Option<&str> {
    block.get("type").and_then(Value::as_str)
}

fn is_whole_read(input: &Value) -> bool {
    input.get("offset").is_none_or(Value::is_null) && input.get("limit").is_none_or(Value::is_null)
}

pub fn assistant_turns_after(messages: &[Value]) -> Vec<usize> {
    let mut after = vec![0; messages.len()];
    let mut seen = 0;
    for (index, message) in messages.iter().enumerate().rev() {
        after[index] = seen;
        if message.get("role").and_then(Value::as_str) == Some("assistant") {
            seen += 1;
        }
    }
    after
}

fn answered_ids(messages: &[Value]) -> HashSet<&str> {
    messages
        .iter()
        .flat_map(blocks)
        .filter(|b| block_type(b) == Some("tool_result"))
        .filter(|b| b.get("is_error").and_then(Value::as_bool) != Some(true))
        .filter_map(|b| b.get("tool_use_id").and_then(Value::as_str))
        .collect()
}

fn read_call<'a>(block: &'a Value, msg_idx: usize, producers: &[String]) -> Option<ReadCall<'a>> {
    if block_type(block) != Some("tool_use") {
        return None;
    }
    let name = block.get("name").and_then(Value::as_str)?;
    if !producers.iter().any(|p| p == name) {
        return None;
    }
    let input = block.get("input")?;
    Some(ReadCall {
        id: block.get("id").and_then(Value::as_str)?,
        path: input.get("file_path").and_then(Value::as_str)?,
        whole: is_whole_read(input),
        msg_idx,
    })
}

pub fn superseded_reads<'a>(messages: &'a [Value], producers: &[String]) -> HashSet<&'a str> {
    let answered = answered_ids(messages);
    let calls: Vec<ReadCall<'a>> = messages
        .iter()
        .enumerate()
        .flat_map(|(idx, m)| blocks(m).filter_map(move |b| read_call(b, idx, producers)))
        .collect();
    let mut latest_whole: HashMap<&str, usize> = HashMap::new();
    for call in calls.iter().filter(|c| c.whole && answered.contains(c.id)) {
        let entry = latest_whole.entry(call.path).or_insert(call.msg_idx);
        *entry = (*entry).max(call.msg_idx);
    }
    calls
        .iter()
        .filter(|c| {
            latest_whole
                .get(c.path)
                .is_some_and(|latest| *latest > c.msg_idx)
        })
        .map(|c| c.id)
        .collect()
}

const EDIT_TOOLS: [&str; 4] = ["Edit", "MultiEdit", "Write", "NotebookEdit"];

fn edit_path(block: &Value) -> Option<(&str, &str)> {
    if block_type(block) != Some("tool_use") {
        return None;
    }
    let name = block.get("name").and_then(Value::as_str)?;
    if !EDIT_TOOLS.contains(&name) {
        return None;
    }
    let input = block.get("input")?;
    let path = input
        .get("file_path")
        .or_else(|| input.get("notebook_path"))
        .and_then(Value::as_str)?;
    Some((block.get("id").and_then(Value::as_str)?, path))
}

pub fn stale_reads<'a>(messages: &'a [Value], producers: &[String]) -> HashSet<&'a str> {
    let answered = answered_ids(messages);
    let mut last_edit: HashMap<&str, usize> = HashMap::new();
    for (idx, message) in messages.iter().enumerate() {
        for (id, path) in blocks(message).filter_map(edit_path) {
            if answered.contains(id) {
                last_edit.insert(path, idx);
            }
        }
    }
    messages
        .iter()
        .enumerate()
        .flat_map(|(idx, m)| blocks(m).filter_map(move |b| read_call(b, idx, producers)))
        .filter(|c| {
            last_edit
                .get(c.path)
                .is_some_and(|edited| *edited > c.msg_idx)
        })
        .map(|c| c.id)
        .collect()
}

pub fn superseded_text(file_path: &str, result_lines: usize) -> String {
    format!(
        "{SUPERSEDED_MARKER_TAG} file={file_path} result_lines={result_lines}. \
         A later Read of this whole file appears further down this conversation; \
         that result is the current copy of the file."
    )
}
