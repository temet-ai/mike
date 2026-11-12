use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use chrono::{DateTime, Utc};
use regex::Regex;

const MIN_KEY_CHARS: usize = 40;
const KEY_CHARS: usize = 120;
const MIN_REPEATS: usize = 3;
const MIN_MEDIAN_GAP_SECONDS: f64 = 120.0;

static UUID: LazyLock<Regex> = LazyLock::new(|| {
    match Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}") {
        Ok(re) => re,
        Err(e) => panic!("invalid uuid pattern: {e}"),
    }
});
static DIGITS: LazyLock<Regex> = LazyLock::new(|| match Regex::new(r"[0-9]+") {
    Ok(re) => re,
    Err(e) => panic!("invalid digits pattern: {e}"),
});
static SPACES: LazyLock<Regex> = LazyLock::new(|| match Regex::new(r"[ \t\n\r\x0c\x0b]+") {
    Ok(re) => re,
    Err(e) => panic!("invalid whitespace pattern: {e}"),
});

pub fn repeat_key(text: &str) -> Option<String> {
    if text.chars().count() < MIN_KEY_CHARS {
        return None;
    }
    let lowered = text.to_lowercase();
    let without_uuids = UUID.replace_all(&lowered, "");
    let without_digits = DIGITS.replace_all(&without_uuids, "");
    let collapsed = SPACES.replace_all(&without_digits, " ");
    Some(
        collapsed
            .trim_matches(' ')
            .chars()
            .take(KEY_CHARS)
            .collect(),
    )
}

pub fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    if values.len() % 2 == 1 {
        Some(values[mid])
    } else {
        Some((values[mid - 1] + values[mid]) / 2.0)
    }
}

fn gap_seconds(later: DateTime<Utc>, earlier: DateTime<Utc>) -> f64 {
    (later - earlier).num_microseconds().map_or_else(
        || (later - earlier).num_milliseconds() as f64 / 1000.0,
        |micros| micros as f64 / 1_000_000.0,
    )
}

pub fn qualifying_keys<'a>(
    entries: impl IntoIterator<Item = (&'a str, DateTime<Utc>)>,
) -> HashSet<&'a str> {
    let mut groups: HashMap<&str, (usize, DateTime<Utc>, Vec<f64>)> = HashMap::new();
    for (key, at) in entries {
        groups
            .entry(key)
            .and_modify(|(count, previous, gaps)| {
                gaps.push(gap_seconds(at, *previous));
                *count += 1;
                *previous = at;
            })
            .or_insert((1, at, Vec::new()));
    }
    groups
        .into_iter()
        .filter(|(_, (count, _, gaps))| {
            *count >= MIN_REPEATS
                && median(&mut gaps.clone()).is_some_and(|m| m >= MIN_MEDIAN_GAP_SECONDS)
        })
        .map(|(key, _)| key)
        .collect()
}
