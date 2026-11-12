//! Lines a captured prompt block drops before it is hashed or compared:
//! operator-defined volatile patterns (compose_prompt::volatile loads them).

use regex::Regex;

#[derive(Debug, Clone)]
pub(crate) struct VolatilePattern {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) pattern: Regex,
    pub(crate) applies_to_block_index: u8,
    pub(crate) enabled: bool,
}

pub(crate) fn strip_volatile_lines(
    text: &str,
    patterns: &[VolatilePattern],
    block_index: u8,
) -> String {
    if !patterns
        .iter()
        .any(|p| p.enabled && p.applies_to_block_index == block_index)
    {
        return text.to_string();
    }

    let mut kept: Vec<&str> = Vec::new();
    for raw_line in text.split('\n') {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        let drop = patterns.iter().any(|p| {
            p.enabled && p.applies_to_block_index == block_index && p.pattern.is_match(line)
        });
        if !drop {
            kept.push(line);
        }
    }
    kept.join("\n")
}
