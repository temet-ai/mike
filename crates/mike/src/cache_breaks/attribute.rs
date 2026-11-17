use serde_json::{Value, json};

use super::normalize::{self, find_slice};

#[derive(Debug, Clone, Default)]
pub(crate) struct Stages {
    pub(crate) applied: Vec<String>,
    pub(crate) filter_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct Sent {
    pub(crate) request_id: i64,
    pub(crate) sent: Value,
    pub(crate) original: Value,
    pub(crate) stages: Stages,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Chain {
    pub(crate) full: Option<Sent>,
    pub(crate) deltas: Vec<Sent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cause {
    Mike,
    Client,
    Unknown,
}

impl Cause {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Mike => "mike",
            Self::Client => "client",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Region {
    Tools,
    System,
    Messages,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Attribution {
    pub(crate) cause: Cause,
    pub(crate) stage: Option<String>,
    pub(crate) location: &'static str,
    pub(crate) source_request_id: Option<i64>,
    pub(crate) detail: Value,
}

fn unknown(location: &'static str, detail: Value) -> Attribution {
    Attribution {
        cause: Cause::Unknown,
        stage: None,
        location,
        source_request_id: None,
        detail,
    }
}

fn client(location: &'static str, source: i64, detail: Value) -> Attribution {
    Attribution {
        cause: Cause::Client,
        stage: None,
        location,
        source_request_id: Some(source),
        detail,
    }
}

fn applied(sources: &[&Sent]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for name in sources.iter().flat_map(|s| s.stages.applied.iter()) {
        if !names.contains(name) {
            names.push(name.clone());
        }
    }
    names
}

fn preferred(region: Region, names: &[String]) -> Option<String> {
    let fits = |n: &str| match region {
        Region::Tools => n == "proxy_tools",
        Region::System => n == "filter",
        Region::Messages => n != "filter" && n != "proxy_tools",
    };
    names
        .iter()
        .find(|n| fits(n))
        .or_else(|| names.first())
        .cloned()
}

fn mike(region: Region, location: &'static str, source: &Sent, cur: &Sent) -> Attribution {
    let names = applied(&[source, cur]);
    let from = source.stages.filter_reason.clone();
    let to = cur.stages.filter_reason.clone();
    let filter_flipped = region == Region::System && from != to;
    let stage = if filter_flipped {
        Some("filter".to_owned())
    } else {
        preferred(region, &names)
    };
    let mut detail = json!({ "applied_stages": names });
    if filter_flipped {
        detail["filter_from"] = json!(from);
        detail["filter_to"] = json!(to);
    }
    Attribution {
        cause: Cause::Mike,
        stage: Some(stage.unwrap_or_else(|| "unknown_stage".to_owned())),
        location,
        source_request_id: Some(source.request_id),
        detail,
    }
}

fn tool_change(full: &Sent, cur: &Sent) -> Value {
    let before = normalize::tool_names(&full.original);
    let after = normalize::tool_names(&cur.original);
    let added: Vec<&String> = after.iter().filter(|n| !before.contains(n)).collect();
    let removed: Vec<&String> = before.iter().filter(|n| !after.contains(n)).collect();
    json!({ "tools_added": added, "tools_removed": removed })
}

fn first_difference(a: &[Value], b: &[Value]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

fn compare_tools(full: &Sent, cur: &Sent) -> Option<Attribution> {
    if normalize::tools(&full.sent) == normalize::tools(&cur.sent) {
        return None;
    }
    if normalize::tools(&full.original) != normalize::tools(&cur.original) {
        return Some(client("tools", full.request_id, tool_change(full, cur)));
    }
    Some(mike(Region::Tools, "tools", full, cur))
}

fn compare_system(full: &Sent, cur: &Sent) -> Option<Attribution> {
    if normalize::system(&full.sent) == normalize::system(&cur.sent) {
        return None;
    }
    if normalize::system(&full.original) != normalize::system(&cur.original) {
        return Some(client("system", full.request_id, json!({})));
    }
    Some(mike(Region::System, "system", full, cur))
}

fn compare_prefix(
    full: &Sent,
    cur: &Sent,
    cur_sent: &[Value],
    cur_orig: &[Value],
) -> Option<Attribution> {
    let sent = normalize::messages(&full.sent);
    if cur_sent.starts_with(&sent) {
        return None;
    }
    let original = normalize::messages(&full.original);
    let at = first_difference(&sent, cur_sent);
    if !cur_orig.starts_with(&original) {
        return Some(client(
            "messages_prefix",
            full.request_id,
            json!({ "message": at }),
        ));
    }
    let mut found = mike(Region::Messages, "messages_prefix", full, cur);
    found.detail["message"] = json!(at);
    Some(found)
}

fn compare_delta(
    delta: &Sent,
    cur: &Sent,
    cur_sent: &[Value],
    cur_orig: &[Value],
) -> Option<Attribution> {
    let sent = normalize::messages(&delta.sent);
    if find_slice(cur_sent, &sent).is_some() {
        return None;
    }
    let original = normalize::messages(&delta.original);
    if find_slice(cur_orig, &original).is_none() {
        return Some(client(
            "delta",
            delta.request_id,
            json!({ "delta_messages": sent.len() }),
        ));
    }
    Some(mike(Region::Messages, "delta", delta, cur))
}

pub(crate) fn attribute(cur: &Sent, chain: &Chain) -> Attribution {
    if normalize::is_continuation(&cur.sent) {
        return unknown("continuation", json!({}));
    }
    let Some(full) = chain.full.as_ref() else {
        return unknown("chain", json!({ "deltas": chain.deltas.len() }));
    };
    let cur_sent = normalize::messages(&cur.sent);
    let cur_orig = normalize::messages(&cur.original);
    compare_tools(full, cur)
        .or_else(|| compare_system(full, cur))
        .or_else(|| compare_prefix(full, cur, &cur_sent, &cur_orig))
        .or_else(|| {
            chain
                .deltas
                .iter()
                .find_map(|d| compare_delta(d, cur, &cur_sent, &cur_orig))
        })
        .unwrap_or_else(|| unknown("none", json!({ "deltas": chain.deltas.len() })))
}

#[cfg(test)]
#[path = "attribute_tests.rs"]
mod tests;
