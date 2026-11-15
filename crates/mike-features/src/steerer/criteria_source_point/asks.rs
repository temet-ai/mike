use serde_json::Value;

use crate::llm::decisions::text::{head_chars, tail_chars};

pub const HEAD_CHARS: usize = 1_500;
pub const TAIL_CHARS: usize = 4_500;
const DROPPED: &str = "[pasted text dropped]";
const COMMAND_OPEN: &str = "<command-name>";
const SKILL_HEADER: &str = "# /";
const KIND_CHARS: usize = 20;

pub fn fit(asks: &str) -> String {
    let kept = without_pasted_bodies(asks.trim());
    let kept = kept.trim();
    if kept.chars().count() <= HEAD_CHARS + TAIL_CHARS {
        return kept.to_owned();
    }
    format!(
        "{}\n[...]\n{}",
        head_chars(kept, HEAD_CHARS).trim_end(),
        tail_chars(kept, TAIL_CHARS).trim_start()
    )
}

fn without_pasted_bodies(asks: &str) -> String {
    entries(asks)
        .iter()
        .map(|entry| strip_body(entry))
        .collect::<Vec<_>>()
        .join("\n")
}

fn kind_prefix_len(line: &str) -> Option<usize> {
    let rest = line.strip_prefix('[')?;
    let close = rest.find("] ")?;
    let kind = &rest[..close];
    (!kind.is_empty()
        && kind.chars().count() <= KIND_CHARS
        && kind.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
    .then_some(close + 3)
}

fn entries(asks: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in asks.lines() {
        match out.last_mut() {
            Some(last) if kind_prefix_len(line).is_none() => {
                last.push('\n');
                last.push_str(line);
            }
            _ => out.push(line.to_owned()),
        }
    }
    out
}

fn strip_body(entry: &str) -> String {
    let Some(cut) = body_start(entry) else {
        return entry.to_owned();
    };
    let kept = entry[..cut].trim_end();
    if kept.is_empty() {
        DROPPED.to_owned()
    } else {
        format!("{kept}\n{DROPPED}")
    }
}

fn body_start(entry: &str) -> Option<usize> {
    let command = entry.find(COMMAND_OPEN);
    let lines = line_starts(entry).find_map(|at| pasted_from(entry, at));
    command.into_iter().chain(lines).min()
}

fn line_starts(entry: &str) -> impl Iterator<Item = usize> + '_ {
    std::iter::once(0).chain(entry.match_indices('\n').map(|(at, _)| at + 1))
}

fn pasted_from(entry: &str, at: usize) -> Option<usize> {
    let skip = if at == 0 {
        kind_prefix_len(entry).unwrap_or(0)
    } else {
        0
    };
    let from = at + skip;
    let line = &entry[from..];
    let pasted = line.starts_with(SKILL_HEADER)
        || (line.trim_start().starts_with('{')
            && serde_json::from_str::<Value>(line.trim()).is_ok());
    pasted.then_some(from)
}
