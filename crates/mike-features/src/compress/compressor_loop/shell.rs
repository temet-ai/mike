use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::followups::{Sources, result_text};
use super::types::{Arms, Call, Cut, Gaps, Lane, ShellTable, bump, stamp};
use crate::compress::shell_output::command::{CommandShape, command_shape};

const MIN_TOKEN: usize = 8;

fn pattern(source: &str) -> Regex {
    match Regex::new(source) {
        Ok(compiled) => compiled,
        Err(error) => panic!("invalid shell token pattern {source}: {error}"),
    }
}

static SHELL_KEPT: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"^kept output lines ([0-9]+(?:-[0-9]+)?(?:,[0-9]+(?:-[0-9]+)?)*)"));

static TOKEN_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"(?:[\w.@+-]+/)+[\w.@+-]+\.[A-Za-z]\w{0,5}(?::\d+)?",
        r"\b[A-Za-z_]\w*(?:::[A-Za-z_]\w*)+\b",
        r"\b[a-z][a-z0-9]*(?:_[a-z0-9]+){2,}\b",
        r"\b[0-9a-f]{8,40}\b",
    ]
    .into_iter()
    .map(pattern)
    .collect()
});

pub(super) fn parse_shell_cut(cut: &Cut) -> Option<Vec<(i64, i64)>> {
    let found = SHELL_KEPT.captures(cut.detail.as_deref().unwrap_or(""))?;
    let mut ranges = Vec::new();
    for part in found[1].split(',') {
        let (first, last) = part.split_once('-').unwrap_or((part, ""));
        let first: i64 = first.parse().ok()?;
        let last: i64 = if last.is_empty() {
            first
        } else {
            last.parse().ok()?
        };
        ranges.push((first, last));
    }
    Some(ranges)
}

pub(super) fn split_output(content: &Value, kept: &[(i64, i64)]) -> Option<(String, String)> {
    let text = result_text(content)?;
    let mut hidden: Vec<&str> = Vec::new();
    let mut shown: Vec<&str> = Vec::new();
    for (index, line) in text.split('\n').enumerate() {
        let number = index as i64 + 1;
        if kept.iter().any(|(a, b)| *a <= number && number <= *b) {
            shown.push(line);
        } else {
            hidden.push(line);
        }
    }
    Some((hidden.join("\n"), shown.join("\n")))
}

pub(super) fn distinctive_tokens(text: &str) -> BTreeSet<String> {
    TOKEN_PATTERNS
        .iter()
        .flat_map(|compiled| compiled.find_iter(text).map(|found| found.as_str()))
        .filter(|token| token.chars().count() >= MIN_TOKEN)
        .map(str::to_string)
        .collect()
}

pub(super) fn needed_tokens(hidden: &str, shown: &str, command: &str) -> BTreeSet<String> {
    distinctive_tokens(hidden)
        .into_iter()
        .filter(|token| !shown.contains(token.as_str()) && !command.contains(token.as_str()))
        .collect()
}

fn serialised(input: Option<&Value>) -> String {
    match input {
        Some(Value::String(text)) => text.clone(),
        Some(value) => value.to_string(),
        None => "null".to_string(),
    }
}

pub(super) struct ShellFollowups<'a> {
    pub(super) cuts: &'a [Cut],
    pub(super) arms: &'a Arms,
    pub(super) sources: &'a Sources,
    pub(super) calls: &'a [Call],
    pub(super) until: f64,
    pub(super) window: f64,
}

fn later_calls(calls: &[Call]) -> HashMap<Lane, Vec<(f64, i64, String)>> {
    let mut later: HashMap<Lane, Vec<(f64, i64, String)>> = HashMap::new();
    for call in calls {
        if let Some(lane) = call.who.lane() {
            later.entry(lane).or_default().push((
                stamp(call.created_at),
                call.id,
                serialised(call.input.as_ref()),
            ));
        }
    }
    later
}

pub(super) fn shell_harm_table(input: &ShellFollowups) -> (ShellTable, Gaps) {
    let later = later_calls(input.calls);
    let mut table = ShellTable::default();
    let mut seen: HashSet<(Lane, &str)> = HashSet::new();
    let mut gaps = Gaps::new();
    for cut in input.cuts {
        let Some(lane) = cut.who.lane() else {
            bump(&mut gaps, "missing_agent_identity");
            continue;
        };
        if !seen.insert((lane.clone(), cut.wire_id.as_str())) {
            continue;
        }
        let kept = parse_shell_cut(cut);
        let source = input.sources.get(&(lane.clone(), cut.wire_id.clone()));
        let row = input
            .arms
            .get(&cut.who.session_id)
            .and_then(|arm| table.row_mut(arm));
        let (Some(kept), Some(row), Some(source)) = (kept, row, source) else {
            bump(&mut gaps, "missing_source_or_policy");
            continue;
        };
        let Some((hidden, shown)) = split_output(&source.content, &kept) else {
            bump(&mut gaps, "missing_result_text");
            continue;
        };
        let after = stamp(cut.created_at).max(stamp(source.result_at));
        if after + input.window > input.until {
            bump(&mut gaps, "right_censored");
            continue;
        }
        let command = source
            .input
            .as_ref()
            .and_then(|used| used.get("command"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let wanted = needed_tokens(&hidden, &shown, command);
        row.cuts += 1;
        let used_later = !wanted.is_empty()
            && later
                .get(&lane)
                .into_iter()
                .flatten()
                .any(|(at, id, text)| {
                    *id != source.id
                        && after < *at
                        && *at <= after + input.window
                        && wanted.iter().any(|token| text.contains(token.as_str()))
                });
        row.needed_later += u64::from(used_later);
        let by_command = if command_shape(command) == CommandShape::Other {
            table.by_command.entry(command_kind(command)).or_default()
        } else {
            &mut table.left_whole_now
        };
        by_command.cuts += 1;
        by_command.needed_later += u64::from(used_later);
    }
    (table, gaps)
}

pub(super) use crate::compress::shell_output::command::command_kind;
