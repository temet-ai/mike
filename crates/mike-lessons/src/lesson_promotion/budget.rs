use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;

use super::ledger::Ledger;

pub(super) const DEFAULT_CAP: usize = 12;
const MIN_CAP: i64 = 1;
const MAX_CAP: i64 = 100;
const YOUNG_DAYS: i64 = 7;
const MIN_SHARED_WORDS: usize = 2;
const FILLER: [&str; 10] = [
    "the", "a", "an", "of", "and", "for", "to", "in", "on", "with",
];

fn stem(word: &str) -> &str {
    let length = word.len();
    if length > 5 && word.ends_with("ing") {
        &word[..length - 3]
    } else if length > 4 && word.ends_with("ed") {
        &word[..length - 2]
    } else if length > 3 && word.ends_with('s') && !word.ends_with("ss") {
        &word[..length - 1]
    } else {
        word
    }
}

fn purpose_words(topic: &str) -> BTreeSet<String> {
    let plain: String = topic
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect();
    plain
        .split_whitespace()
        .filter(|word| !FILLER.contains(word))
        .map(|word| stem(word).to_owned())
        .collect()
}

pub(super) fn same_purpose(first: &str, second: &str) -> bool {
    let (a, b) = (purpose_words(first), purpose_words(second));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    let shared = a.intersection(&b).count();
    shared >= MIN_SHARED_WORDS && shared * 4 >= a.len().min(b.len()) * 3
}

fn project_of(key: &str) -> &str {
    key.split(':').next().unwrap_or_default()
}

pub(super) fn cap_from_setting(value: Option<&serde_json::Value>) -> usize {
    value
        .and_then(serde_json::Value::as_i64)
        .filter(|number| (MIN_CAP..=MAX_CAP).contains(number))
        .and_then(|number| usize::try_from(number).ok())
        .unwrap_or(DEFAULT_CAP)
}

pub(super) type Candidates = BTreeMap<String, (String, usize)>;

pub(super) fn hold_overlapping(
    candidates: &Candidates,
    shipped: &BTreeSet<String>,
) -> BTreeMap<String, String> {
    let mut by_project: BTreeMap<&str, Vec<&String>> = BTreeMap::new();
    for key in candidates.keys() {
        by_project.entry(project_of(key)).or_default().push(key);
    }
    let mut held = BTreeMap::new();
    for mut keys in by_project.into_values() {
        keys.sort_by_key(|key| {
            (
                !shipped.contains(*key),
                std::cmp::Reverse(candidates[*key].1),
                (*key).clone(),
            )
        });
        let mut survivors: Vec<&String> = Vec::new();
        for key in keys {
            let kept = survivors
                .iter()
                .find(|survivor| same_purpose(&candidates[**survivor].0, &candidates[key].0));
            match kept {
                Some(survivor) => {
                    held.insert(key.clone(), (*survivor).clone());
                }
                None => survivors.push(key),
            }
        }
    }
    held
}

pub(super) struct CapInputs<'a> {
    pub ledger: &'a Ledger,
    pub loads: &'a BTreeMap<String, i64>,
    pub today: NaiveDate,
    pub live: &'a BTreeSet<String>,
    pub fresh: &'a BTreeSet<String>,
    pub lessons: &'a BTreeMap<String, usize>,
    pub reviewed: &'a BTreeSet<String>,
}

impl CapInputs<'_> {
    fn first_emitted(&self, key: &str) -> &str {
        self.ledger
            .get(key)
            .map_or("", |entry| entry.first_emitted.as_str())
    }

    fn age_days(&self, key: &str) -> i64 {
        NaiveDate::parse_from_str(self.first_emitted(key), "%Y-%m-%d")
            .map_or(0, |first| (self.today - first).num_days())
    }

    fn evidence(&self, key: &String) -> (i64, String, String) {
        (
            self.loads.get(key).copied().unwrap_or(0),
            self.first_emitted(key).to_owned(),
            key.clone(),
        )
    }
}

pub(super) fn enforce_cap(inputs: &CapInputs<'_>, cap: usize) -> (Vec<String>, Vec<String>) {
    let mut by_project: BTreeMap<&str, Vec<&String>> = BTreeMap::new();
    for key in inputs.live {
        by_project.entry(project_of(key)).or_default().push(key);
    }
    let (mut retire, mut hold) = (Vec::new(), Vec::new());
    for keys in by_project.values() {
        let Some(overflow) = keys.len().checked_sub(cap).filter(|over| *over > 0) else {
            continue;
        };
        let (retired, held) = settle_project(inputs, keys, overflow);
        retire.extend(retired);
        hold.extend(held);
    }
    (retire, hold)
}

fn settle_project(
    inputs: &CapInputs<'_>,
    keys: &[&String],
    overflow: usize,
) -> (Vec<String>, Vec<String>) {
    let fresh = |key: &String| inputs.fresh.contains(key);
    let reviewed = |key: &String| inputs.reviewed.contains(key);
    let reviewed_newcomers = keys.iter().filter(|k| fresh(k) && reviewed(k)).count();
    let mut generated: Vec<&String> = keys
        .iter()
        .copied()
        .filter(|k| !fresh(k) && !reviewed(k))
        .collect();
    generated.sort_by_key(|key| inputs.evidence(key));
    let mut mature: Vec<&String> = keys
        .iter()
        .copied()
        .filter(|k| !fresh(k) && reviewed(k) && inputs.age_days(k) >= YOUNG_DAYS)
        .collect();
    mature.sort_by_key(|key| inputs.evidence(key));
    let displaced_count = overflow.min(reviewed_newcomers);
    let displaced = &generated[..displaced_count.min(generated.len())];
    let rest = overflow - displaced.len();
    let aged: Vec<&String> = generated[displaced.len()..]
        .iter()
        .copied()
        .filter(|k| inputs.age_days(k) >= YOUNG_DAYS)
        .chain(mature)
        .collect();
    let taken = rest.min(aged.len());
    let retire: Vec<String> = displaced
        .iter()
        .chain(&aged[..taken])
        .map(|key| (*key).clone())
        .collect();
    let remaining = rest - taken;
    let mut newcomers: Vec<&String> = keys.iter().copied().filter(|k| fresh(k)).collect();
    newcomers.sort_by_key(|key| {
        (
            reviewed(key),
            inputs.lessons.get(*key).copied().unwrap_or(0),
            (*key).clone(),
        )
    });
    let hold = newcomers.into_iter().take(remaining).cloned().collect();
    (retire, hold)
}

#[cfg(test)]
#[path = "budget_tests.rs"]
mod budget_tests;
