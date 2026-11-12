//! Which file reads in a session repeat an earlier read: the index the request
//! store keeps per session and the observe/rewrite logic over tool uses.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub(crate) struct DedupIndexEntry {
    pub(crate) tool_use_id: String,
    pub(crate) timestamp: DateTime<Utc>,
    pub(crate) byte_len: u64,
}

pub(crate) type SessionDedupIndex = HashMap<(String, String), DedupIndexEntry>;

#[derive(Debug, Clone, Default)]
pub(crate) struct PathDedupStats {
    pub(crate) hit_count: u64,
    pub(crate) would_save_bytes: u64,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SessionDedupStats {
    pub(crate) hit_count: u64,
    pub(crate) would_save_bytes: u64,
    pub(crate) read_bytes_total: u64,
    pub(crate) per_path: HashMap<String, PathDedupStats>,
}

#[derive(Debug, Clone)]
pub(crate) struct DedupHit {
    pub(crate) file_path: String,
    pub(crate) prior_tool_use_id: String,
    pub(crate) prior_timestamp: DateTime<Utc>,
    pub(crate) byte_len: u64,
}

enum ToolShape<'a> {
    ReadLike(String),
    Invalidates(String),
    BashCommand(&'a str),
    Irrelevant,
}

fn classify<'a>(name: &str, input: &'a Value) -> ToolShape<'a> {
    match name {
        "Read" => match input.get("file_path").and_then(Value::as_str) {
            Some(p) => ToolShape::ReadLike(p.to_owned()),
            None => ToolShape::Irrelevant,
        },
        "Edit" | "Write" | "NotebookEdit" => match input.get("file_path").and_then(Value::as_str) {
            Some(p) => ToolShape::Invalidates(p.to_owned()),
            None => ToolShape::Irrelevant,
        },
        "Bash" => match input.get("command").and_then(Value::as_str) {
            Some(command) => match parse_cat_path(command) {
                Some(path) => ToolShape::ReadLike(path),
                None => ToolShape::BashCommand(command),
            },
            None => ToolShape::Irrelevant,
        },
        _ => ToolShape::Irrelevant,
    }
}

fn parse_cat_path(command: &str) -> Option<String> {
    let rest = command.trim().strip_prefix("cat ")?.trim();
    let path = rest.strip_prefix("-n ").unwrap_or(rest).trim();
    if path.is_empty() || path.contains(['|', '>', '<', '&', ';', ' ']) {
        return None;
    }
    Some(path.trim_matches('"').trim_matches('\'').to_owned())
}

fn invalidate_path(index: &mut SessionDedupIndex, file_path: &str) {
    index.retain(|(path, _), _| path != file_path);
}

const MAX_DEDUP_INDEX_ENTRIES_PER_SESSION: usize = 2_000;

fn enforce_cap(index: &mut SessionDedupIndex) {
    if index.len() <= MAX_DEDUP_INDEX_ENTRIES_PER_SESSION {
        return;
    }
    if let Some(oldest_key) = index
        .iter()
        .min_by_key(|(_, entry)| entry.timestamp)
        .map(|(key, _)| key.clone())
    {
        index.remove(&oldest_key);
    }
}

pub(crate) fn stub_text(hit: &DedupHit) -> String {
    format!(
        "[proxy: duplicate read suppressed — {file_path} unchanged since tool_use_id \
         {prior_tool_use_id} at {prior_timestamp}, which already returned this content in \
         this session. Refer to that turn's result. If you need to re-verify, re-issue with \
         force=true or after editing the file.]",
        file_path = hit.file_path,
        prior_tool_use_id = hit.prior_tool_use_id,
        prior_timestamp = hit.prior_timestamp.to_rfc3339(),
    )
}

fn index_tool_uses(blocks: &[Value]) -> HashMap<&str, (&str, &Value)> {
    let mut use_by_id: HashMap<&str, (&str, &Value)> = HashMap::new();
    for block in blocks {
        if block.get("type").and_then(Value::as_str) != Some("tool_use") {
            continue;
        }
        let Some(id) = block.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(name) = block.get("name").and_then(Value::as_str) else {
            continue;
        };
        use_by_id.insert(id, (name, block.get("input").unwrap_or(&Value::Null)));
    }
    use_by_id
}

struct FoldCtx<'a> {
    index: &'a mut SessionDedupIndex,
    stats: &'a mut SessionDedupStats,
    now: DateTime<Utc>,
    armed: bool,
}

fn record_read_like(
    file_path: String,
    block: &Value,
    tool_use_id: &str,
    ctx: &mut FoldCtx<'_>,
) -> (Option<DedupHit>, Option<String>) {
    let content = block.get("content").cloned().unwrap_or(Value::Null);
    let Ok(bytes) = serde_json::to_vec(&content) else {
        return (None, None);
    };
    let byte_len = bytes.len() as u64;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let sha = hex::encode(hasher.finalize());

    ctx.stats.read_bytes_total += byte_len;
    let key = (file_path.clone(), sha);
    let hit = ctx.index.get(&key).map(|prior| DedupHit {
        file_path: file_path.clone(),
        prior_tool_use_id: prior.tool_use_id.clone(),
        prior_timestamp: prior.timestamp,
        byte_len: prior.byte_len,
    });
    let stub = if let Some(hit) = &hit {
        ctx.stats.hit_count += 1;
        ctx.stats.would_save_bytes += byte_len;
        let path_stats = ctx.stats.per_path.entry(file_path.clone()).or_default();
        path_stats.hit_count += 1;
        path_stats.would_save_bytes += byte_len;
        ctx.armed.then(|| stub_text(hit))
    } else {
        ctx.stats.per_path.entry(file_path.clone()).or_default();
        None
    };
    ctx.index.insert(
        key,
        DedupIndexEntry {
            tool_use_id: tool_use_id.to_owned(),
            timestamp: ctx.now,
            byte_len,
        },
    );
    enforce_cap(ctx.index);
    (hit, stub)
}

fn invalidate_paths_in_command(index: &mut SessionDedupIndex, command: &str) {
    let stale: Vec<String> = index
        .keys()
        .map(|(path, _)| path.clone())
        .filter(|path| command.contains(path.as_str()))
        .collect();
    for path in stale {
        invalidate_path(index, &path);
    }
}

fn fold_tool_results(
    tool_results: &[Value],
    use_by_id: &HashMap<&str, (&str, &Value)>,
    ctx: &mut FoldCtx<'_>,
) -> (Vec<DedupHit>, Vec<(usize, String)>) {
    let mut hits = Vec::new();
    let mut stub_targets: Vec<(usize, String)> = Vec::new();
    for (content_index, block) in tool_results.iter().enumerate() {
        if block.get("type").and_then(Value::as_str) != Some("tool_result") {
            continue;
        }
        let Some(tool_use_id) = block.get("tool_use_id").and_then(Value::as_str) else {
            continue;
        };
        let Some(&(name, input)) = use_by_id.get(tool_use_id) else {
            continue;
        };

        match classify(name, input) {
            ToolShape::Invalidates(file_path) => invalidate_path(ctx.index, &file_path),
            ToolShape::BashCommand(command) => invalidate_paths_in_command(ctx.index, command),
            ToolShape::ReadLike(file_path) => {
                let (hit, stub) = record_read_like(file_path, block, tool_use_id, ctx);
                if let Some(hit) = hit {
                    if let Some(stub) = stub {
                        stub_targets.push((content_index, stub));
                    }
                    hits.push(hit);
                }
            }
            ToolShape::Irrelevant => {}
        }
    }
    (hits, stub_targets)
}

fn splice_stub_targets(
    body: &Value,
    user_msg_index: usize,
    stub_targets: &[(usize, String)],
) -> Option<Value> {
    if stub_targets.is_empty() {
        return None;
    }
    let mut new_body = body.clone();
    for (content_index, stub) in stub_targets {
        let pointer = format!("/messages/{user_msg_index}/content/{content_index}/content");
        if let Some(slot) = new_body.pointer_mut(&pointer) {
            *slot = Value::String(stub.clone());
        }
    }
    Some(new_body)
}

fn fold(
    body: &Value,
    index: &mut SessionDedupIndex,
    stats: &mut SessionDedupStats,
    now: DateTime<Utc>,
    armed: bool,
) -> (Vec<DedupHit>, Option<Value>) {
    let Some(msgs) = crate::extract::request::messages_ignoring_trailing_system(body) else {
        return (Vec::new(), None);
    };
    if msgs.len() < 2 {
        return (Vec::new(), None);
    }
    let user_msg_index = msgs.len() - 1;
    let assistant_msg = &msgs[msgs.len() - 2];
    let user_msg = &msgs[user_msg_index];
    if assistant_msg.get("role").and_then(Value::as_str) != Some("assistant")
        || user_msg.get("role").and_then(Value::as_str) != Some("user")
    {
        return (Vec::new(), None);
    }
    let Some(tool_uses) = assistant_msg.get("content").and_then(Value::as_array) else {
        return (Vec::new(), None);
    };
    let Some(tool_results) = user_msg.get("content").and_then(Value::as_array) else {
        return (Vec::new(), None);
    };

    let use_by_id = index_tool_uses(tool_uses);
    if use_by_id.is_empty() {
        return (Vec::new(), None);
    }

    let mut ctx = FoldCtx {
        index,
        stats,
        now,
        armed,
    };
    let (hits, stub_targets) = fold_tool_results(tool_results, &use_by_id, &mut ctx);
    let rewritten = splice_stub_targets(body, user_msg_index, &stub_targets);
    (hits, rewritten)
}

pub(crate) fn observe(
    body: &Value,
    index: &mut SessionDedupIndex,
    stats: &mut SessionDedupStats,
    now: DateTime<Utc>,
) -> Vec<DedupHit> {
    fold(body, index, stats, now, false).0
}

pub(crate) fn rewrite(
    body: &Value,
    index: &mut SessionDedupIndex,
    stats: &mut SessionDedupStats,
    now: DateTime<Utc>,
) -> (Vec<DedupHit>, Option<Value>) {
    fold(body, index, stats, now, true)
}

#[cfg(test)]
#[path = "read_dedup_tests_support.rs"]
mod tests_support;

#[cfg(test)]
#[path = "read_dedup_tests_observe.rs"]
mod tests_observe;

#[cfg(test)]
#[path = "read_dedup_tests_rewrite.rs"]
mod tests_rewrite;
