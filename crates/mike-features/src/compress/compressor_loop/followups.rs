use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::types::{Arms, Call, Cut, ExposureRow, Gaps, HarmTable, Lane, Source, bump, stamp};

pub(super) type Kept = Option<Vec<(i64, i64)>>;
pub(super) type Sources = HashMap<(Lane, String), Source>;

fn pattern(source: &str) -> Regex {
    match Regex::new(source) {
        Ok(compiled) => compiled,
        Err(error) => panic!("invalid compressor loop pattern {source}: {error}"),
    }
}

static NUMBERED: LazyLock<Regex> = LazyLock::new(|| pattern(r"^\s*([0-9]+)\t"));
static KEPT_LINES: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"^file (.+?): kept file lines ([0-9]+)-([0-9]+) and ([0-9]+)-([0-9]+)")
});
static SUPERSEDED_READ: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"^file (.+?): a later whole read"));

const SUPERSEDED_REASON: &str = "file_read_superseded";

pub(super) fn file_type(path: &str) -> String {
    let name = path.rsplit('/').find(|part| !part.is_empty()).unwrap_or("");
    match name.rfind('.') {
        Some(dot) if dot > 0 && dot + 1 < name.len() => name[dot..].to_lowercase(),
        _ => "(none)".to_string(),
    }
}

pub(super) fn anchors(call: &Call) -> Vec<String> {
    let text = |value: &Value| {
        value
            .get("old_string")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let empty = Value::Null;
    let data = call.input.as_ref().unwrap_or(&empty);
    if call.tool_name == "MultiEdit" {
        data.get("edits")
            .and_then(Value::as_array)
            .map(|edits| edits.iter().map(text).collect())
            .unwrap_or_default()
    } else {
        vec![text(data)]
    }
}

pub(super) fn result_text(content: &Value) -> Option<String> {
    match content {
        Value::String(text) => Some(text.clone()),
        Value::Array(parts) => Some(
            parts
                .iter()
                .filter(|part| part.is_object())
                .map(|part| part.get("text").and_then(Value::as_str).unwrap_or(""))
                .collect(),
        ),
        _ => None,
    }
}

pub(super) fn hidden_text(content: &Value, kept: &Kept) -> Option<(String, String)> {
    let text = result_text(content)?;
    let mut hidden: Vec<&str> = Vec::new();
    let mut shown: Vec<&str> = Vec::new();
    for (index, line) in text.split('\n').enumerate() {
        let (number, body) = match NUMBERED.captures(line) {
            Some(found) => {
                let end = found.get(0).map_or(0, |whole| whole.end());
                let number = found[1].parse::<i64>().unwrap_or(i64::MAX);
                (number, &line[end..])
            }
            None => (index as i64 + 1, line),
        };
        let inside = kept
            .as_ref()
            .is_some_and(|ranges| ranges.iter().any(|(a, b)| *a <= number && number <= *b));
        if inside {
            shown.push(body);
        } else {
            hidden.push(body);
        }
    }
    Some((hidden.join("\n"), shown.join("\n")))
}

pub(super) fn parse_cut(cut: &Cut) -> Option<(String, Kept)> {
    let detail = cut.detail.as_deref().unwrap_or("");
    if let Some(found) = KEPT_LINES.captures(detail) {
        let kept = if cut.reason == SUPERSEDED_REASON {
            None
        } else {
            let number = |group: usize| found[group].parse::<i64>().ok();
            Some(vec![(number(2)?, number(3)?), (number(4)?, number(5)?)])
        };
        return Some((found[1].to_string(), kept));
    }
    let found = SUPERSEDED_READ.captures(detail)?;
    (cut.reason == SUPERSEDED_REASON).then(|| (found[1].to_string(), None))
}

pub(super) fn pick_sources(rows: Vec<Source>) -> Sources {
    let mut sources: Sources = HashMap::new();
    let mut ambiguous: HashSet<(Lane, String)> = HashSet::new();
    for row in rows {
        let Some(lane) = row.who.lane() else {
            continue;
        };
        let key = (lane, row.wire_id.clone());
        match sources.get(&key) {
            Some(held) if held.id != row.id => {
                ambiguous.insert(key);
            }
            Some(held) if stamp(row.result_at) >= stamp(held.result_at) => {}
            _ => {
                sources.insert(key, row);
            }
        }
    }
    for key in ambiguous {
        sources.remove(&key);
    }
    sources
}

pub(super) struct Followups<'a> {
    pub(super) cuts: &'a [Cut],
    pub(super) arms: &'a Arms,
    pub(super) calls: &'a [Call],
    pub(super) sources: &'a Sources,
    pub(super) until: f64,
    pub(super) window: f64,
}

fn index_calls(calls: &[Call]) -> HashMap<(Lane, String), Vec<&Call>> {
    let mut by_file: HashMap<(Lane, String), Vec<&Call>> = HashMap::new();
    for call in calls {
        if let (Some(lane), Some(path)) = (call.who.lane(), call.file_path.as_ref()) {
            by_file.entry((lane, path.clone())).or_default().push(call);
        }
    }
    by_file
}

fn edits_hidden(call: &Call, hidden: &str, shown: &str) -> bool {
    matches!(call.tool_name.as_str(), "Edit" | "MultiEdit")
        && anchors(call)
            .iter()
            .any(|anchor| !anchor.is_empty() && hidden.contains(anchor) && !shown.contains(anchor))
}

#[derive(Default)]
struct Seen {
    reads: BTreeMap<(String, String), HashSet<i64>>,
    edits: BTreeMap<(String, String), HashSet<i64>>,
}

pub(super) fn analyze(input: &Followups) -> (HarmTable, Gaps) {
    let by_file = index_calls(input.calls);
    let mut table = HarmTable::new();
    let mut seen_keys: HashSet<(Lane, &str)> = HashSet::new();
    let mut seen = Seen::default();
    let mut gaps = Gaps::new();
    for cut in input.cuts {
        let Some(lane) = cut.who.lane() else {
            bump(&mut gaps, "missing_agent_identity");
            continue;
        };
        if !seen_keys.insert((lane.clone(), cut.wire_id.as_str())) {
            continue;
        }
        let source = input.sources.get(&(lane.clone(), cut.wire_id.clone()));
        let arm = input.arms.get(&cut.who.session_id);
        let (Some((path, kept)), Some(arm), Some(source)) = (parse_cut(cut), arm, source) else {
            bump(&mut gaps, "missing_source_or_policy");
            continue;
        };
        if source.file_path.as_deref() != Some(path.as_str()) {
            bump(&mut gaps, "source_path_mismatch");
            continue;
        }
        let Some((hidden, shown)) = hidden_text(&source.content, &kept) else {
            bump(&mut gaps, "missing_result_text");
            continue;
        };
        let after = stamp(cut.created_at).max(stamp(source.result_at));
        if after + input.window > input.until {
            bump(&mut gaps, "right_censored");
            continue;
        }
        let bucket = (file_type(&path), arm.clone());
        let later: Vec<&Call> = by_file
            .get(&(lane, path.clone()))
            .into_iter()
            .flatten()
            .copied()
            .filter(|call| {
                let at = stamp(call.created_at);
                call.id != source.id && after < at && at <= after + input.window
            })
            .collect();
        let reads: HashSet<i64> = later
            .iter()
            .filter(|call| call.tool_name == "Read")
            .map(|call| call.id)
            .collect();
        let edits: HashSet<i64> = later
            .iter()
            .filter(|call| edits_hidden(call, &hidden, &shown))
            .map(|call| call.id)
            .collect();
        record(&mut table, &mut seen, bucket, reads, edits);
    }
    (table, gaps)
}

fn record(
    table: &mut HarmTable,
    seen: &mut Seen,
    bucket: (String, String),
    reads: HashSet<i64>,
    edits: HashSet<i64>,
) {
    let row: &mut ExposureRow = table.entry(bucket.clone()).or_default();
    row.cuts += 1;
    row.exposures_with_hidden_edit += u64::from(!edits.is_empty());
    row.exposures_with_reread += u64::from(!reads.is_empty());
    let read_union = seen.reads.entry(bucket.clone()).or_default();
    read_union.extend(reads);
    row.reread = read_union.len() as u64;
    let edit_union = seen.edits.entry(bucket).or_default();
    edit_union.extend(edits);
    row.edit_hidden = edit_union.len() as u64;
}
