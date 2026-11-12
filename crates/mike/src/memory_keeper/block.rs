use crate::wire::manifest::sha256_hex;

use super::resolve::{Pick, Resolution};
use super::snapshot::ProjectThreads;

pub(crate) const KEEPER_SOURCE: &str = "memory-keeper";
pub(crate) const KEEPER_MARKER_OPEN: &str = "[keeper:";
const HASH_CHARS: usize = 12;
const SUMMARY_CHARS: usize = 120;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Block {
    pub(crate) text: String,
    pub(crate) hash: String,
}

pub(crate) fn render(threads: &ProjectThreads, resolution: &Resolution) -> Option<Block> {
    let body = match resolution {
        Resolution::One(pick) => threads.entry(&pick.slug)?.text.to_string(),
        Resolution::Several(picks) => several(threads, picks),
        Resolution::None => return None,
    };
    let hash = short_hash(&body);
    let text = format!(
        "<system-reminder source=\"{KEEPER_SOURCE}\">\n{KEEPER_MARKER_OPEN}{hash}]\n{body}\n</system-reminder>"
    );
    Some(Block { text, hash })
}

fn several(threads: &ProjectThreads, picks: &[Pick]) -> String {
    let mut lines = vec!["Open work threads this prompt may refer to:".to_owned()];
    for pick in picks {
        let Some(entry) = threads.entry(&pick.slug) else {
            continue;
        };
        let summary = summary(&entry.facts.goal, &entry.facts.title);
        lines.push(format!(
            "- {} ({}): {summary} [thread:{}]",
            pick.slug, entry.facts.status, pick.slug
        ));
    }
    lines.push("Each thread's full state: work_thread_get <slug>.".to_owned());
    lines.join("\n")
}

fn summary(goal: &str, title: &str) -> String {
    let source = if goal.trim().is_empty() { title } else { goal };
    let flat = source.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= SUMMARY_CHARS {
        return flat;
    }
    let cut: String = flat.chars().take(SUMMARY_CHARS - 1).collect();
    format!("{}…", cut.trim_end())
}

fn short_hash(body: &str) -> String {
    sha256_hex(body.as_bytes())
        .chars()
        .take(HASH_CHARS)
        .collect()
}

#[cfg(test)]
#[path = "block_tests.rs"]
mod tests;
