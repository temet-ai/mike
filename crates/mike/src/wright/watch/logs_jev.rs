use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::logs::LogLine;
use crate::llm::decisions::Question;
use crate::llm::decisions::env::Pace;
use crate::llm::decisions::point::{Asked, Decider, noul};
use crate::llm::decisions::record::DecisionSubject;
use crate::llm::decisions::text::head_chars;

pub(crate) const POINT: &str = "log_group";
pub(crate) const QUESTION_SET: &str = "log_group_v1";
const SAME: &str = "same";
const MERGE_AT: f64 = 0.8;
const MAX_ASKS: usize = 20;
const MESSAGE_CHARS: usize = 300;
const WORDS_SHARED: f64 = 0.5;

pub(crate) type Key = (String, String);
pub(crate) type Groups<'a> = BTreeMap<Key, Vec<&'a LogLine>>;

fn words(shape: &str) -> BTreeSet<&str> {
    shape.split_whitespace().collect()
}

pub(crate) fn similar(a: &str, b: &str) -> bool {
    let (left, right) = (words(a), words(b));
    let union = left.union(&right).count();
    union > 0 && (left.intersection(&right).count() as f64 / union as f64) >= WORDS_SHARED
}

pub(crate) fn candidate_pairs(
    groups: &[(Key, Vec<&LogLine>)],
    repeats: usize,
) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    for (at, (key_a, hits_a)) in groups.iter().enumerate() {
        for (offset, (key_b, hits_b)) in groups[at + 1..].iter().enumerate() {
            let below = hits_a.len() < repeats || hits_b.len() < repeats;
            if key_a.0 == key_b.0 && below && similar(&key_a.1, &key_b.1) {
                pairs.push((at, at + 1 + offset));
            }
        }
    }
    pairs.sort_by_key(|(a, b)| std::cmp::Reverse(groups[*a].1.len() + groups[*b].1.len()));
    pairs
}

pub(crate) fn questions() -> BTreeMap<String, Question> {
    BTreeMap::from([(
        SAME.to_owned(),
        noul(
            "Two log lines from the same component were not grouped by the usual rule. Do they report the same underlying problem, differing only in details such as names, numbers or identifiers?",
            "They report the same problem.",
            "They report different problems.",
        ),
    )])
}

fn described(key: &Key, hits: &[&LogLine]) -> Value {
    json!({
        "target": key.0,
        "message": hits.last().map(|line| head_chars(&line.message, MESSAGE_CHARS)),
        "lines_like_it": hits.len(),
    })
}

pub(crate) fn state(a: (&Key, &[&LogLine]), b: (&Key, &[&LogLine])) -> Value {
    json!({"line_a": described(a.0, a.1), "line_b": described(b.0, b.1)})
}

pub(crate) fn compose(asked: &Asked) -> Option<f64> {
    if asked.failed() {
        return None;
    }
    asked.noul(SAME)
}

pub(crate) fn merged<'a>(
    groups: Vec<(Key, Vec<&'a LogLine>)>,
    links: &[(usize, usize)],
) -> Groups<'a> {
    let mut root: Vec<usize> = (0..groups.len()).collect();
    for (a, b) in links {
        let (ra, rb) = (find(&root, *a), find(&root, *b));
        if ra != rb {
            let (keep, fold) = if groups[ra].1.len() >= groups[rb].1.len() {
                (ra, rb)
            } else {
                (rb, ra)
            };
            root[fold] = keep;
        }
    }
    let mut out: BTreeMap<usize, (Key, Vec<&'a LogLine>)> = BTreeMap::new();
    for (at, (key, hits)) in groups.into_iter().enumerate() {
        let top = find(&root, at);
        let entry = out.entry(top).or_insert_with(|| (key.clone(), Vec::new()));
        if top == at {
            entry.0 = key;
        }
        entry.1.extend(hits);
    }
    out.into_values()
        .map(|(key, mut hits)| {
            hits.sort_by_key(|line| line.at);
            (key, hits)
        })
        .collect()
}

fn find(root: &[usize], mut at: usize) -> usize {
    while root[at] != at {
        at = root[at];
    }
    at
}

async fn ask_pair(
    decider: &Decider,
    groups: &[(Key, Vec<&LogLine>)],
    (a, b): (usize, usize),
    pace: &mut Pace,
) -> bool {
    let asked = decider
        .ask(
            QUESTION_SET,
            state((&groups[a].0, &groups[a].1), (&groups[b].0, &groups[b].1)),
            questions(),
            DecisionSubject::default(),
            json!({"merge_at": MERGE_AT}),
        )
        .await;
    pace.spent(&asked);
    let probability = compose(&asked);
    let merge = probability.is_some_and(|p| p >= MERGE_AT) && decider.applies();
    decider
        .mark(
            &asked,
            asked.composed(json!({"score": probability.unwrap_or(1.0)})),
            if merge { "chosen" } else { "none" },
        )
        .await;
    merge
}

pub(crate) async fn merge<'a>(decider: &Decider, groups: Groups<'a>, repeats: usize) -> Groups<'a> {
    let listed: Vec<(Key, Vec<&'a LogLine>)> = groups.into_iter().collect();
    let mut pace = Pace::new(MAX_ASKS);
    let mut links = Vec::new();
    for pair in candidate_pairs(&listed, repeats) {
        if !pace.may_ask() {
            break;
        }
        if ask_pair(decider, &listed, pair, &mut pace).await {
            links.push(pair);
        }
    }
    merged(listed, &links)
}

#[cfg(test)]
#[path = "logs_jev_tests.rs"]
mod tests;
