use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;
use std::time::Duration;

use dashmap::DashMap;
use serde_json::{Value, json};

use super::logs::LogLine;
use crate::llm::decisions::Question;
use crate::llm::decisions::env::Pace;
use crate::llm::decisions::point::{Asked, Decider, noul};
use crate::llm::decisions::record::DecisionSubject;
use crate::llm::decisions::reuse::recent_answers;
use crate::llm::decisions::text::head_chars;

pub(crate) const POINT: &str = "log_group";
pub(crate) const QUESTION_SET: &str = "log_group_v2";
const SAME: &str = "same";
const MERGE_AT: f64 = 0.8;
const KEEP_APART: f64 = 0.3;
const MAX_ASKS: usize = 20;
const MESSAGE_CHARS: usize = 300;
const WORDS_SHARED: f64 = 0.5;
const SLOW_STATEMENT_PREFIX: &str = "slow statement: execution time exceeded alert threshold:";
const MAX_REMEMBERED: usize = 20_000;
const REUSE_FOR: Duration = Duration::from_secs(7 * 24 * 3600);

pub(crate) type Key = (String, String);
pub(crate) type Groups<'a> = BTreeMap<Key, Vec<&'a LogLine>>;
type MemoKey = (String, String, String);

static MEMO: LazyLock<DashMap<MemoKey, f64>> = LazyLock::new(DashMap::new);

fn words(shape: &str) -> BTreeSet<&str> {
    shape.split_whitespace().collect()
}

pub(crate) fn similar(a: &str, b: &str) -> bool {
    let (left, right) = (words(a), words(b));
    let union = left.union(&right).count();
    union > 0 && (left.intersection(&right).count() as f64 / union as f64) >= WORDS_SHARED
}

pub(crate) fn distinct<'a>(a: &'a str, b: &'a str) -> (&'a str, &'a str) {
    let known = if a.starts_with(SLOW_STATEMENT_PREFIX) && b.starts_with(SLOW_STATEMENT_PREFIX) {
        SLOW_STATEMENT_PREFIX.len()
    } else {
        0
    };
    (a[known..].trim_start(), b[known..].trim_start())
}

fn similar_in_the_distinct_text(a: &str, b: &str) -> bool {
    let (left, right) = distinct(a, b);
    similar(left, right)
}

pub(crate) fn candidate_pairs(
    groups: &[(Key, Vec<&LogLine>)],
    repeats: usize,
) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    for (at, (key_a, hits_a)) in groups.iter().enumerate() {
        for (offset, (key_b, hits_b)) in groups[at + 1..].iter().enumerate() {
            let below = hits_a.len() < repeats || hits_b.len() < repeats;
            if key_a.0 == key_b.0 && below && similar_in_the_distinct_text(&key_a.1, &key_b.1) {
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
            "Do `line_a.message` and `line_b.message` report the same problem?",
            "The same statement or error; only names, numbers or identifiers differ",
            "Different SQL statements, tables or error kinds",
        ),
    )])
}

pub(crate) fn state(a: &Key, b: &Key) -> Value {
    let (left, right) = distinct(&a.1, &b.1);
    json!({
        "line_a": {"target": a.0, "message": head_chars(left, MESSAGE_CHARS)},
        "line_b": {"target": b.0, "message": head_chars(right, MESSAGE_CHARS)},
    })
}

fn memo_key(a: &Key, b: &Key) -> MemoKey {
    let (first, second) = if a.1 <= b.1 {
        (&a.1, &b.1)
    } else {
        (&b.1, &a.1)
    };
    (a.0.clone(), first.clone(), second.clone())
}

fn remember(key: MemoKey, probability: f64) {
    if MEMO.len() > MAX_REMEMBERED {
        MEMO.clear();
    }
    MEMO.insert(key, probability);
}

async fn remembered(decider: &Decider, a: &Key, b: &Key) -> Option<f64> {
    let key = memo_key(a, b);
    if let Some(known) = MEMO.get(&key) {
        return Some(*known);
    }
    let (_, asked) = recent_answers(
        decider.persist(),
        POINT,
        QUESTION_SET,
        &state(a, b),
        REUSE_FOR,
    )
    .await?;
    let probability = compose(&asked)?;
    remember(key, probability);
    Some(probability)
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

pub(crate) fn applied_for(probability: Option<f64>, merge: bool) -> &'static str {
    match probability {
        _ if merge => "chosen",
        Some(p) if p < KEEP_APART => "skipped",
        _ => "none",
    }
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
            state(&groups[a].0, &groups[b].0),
            questions(),
            DecisionSubject::default(),
            json!({"merge_at": MERGE_AT, "keep_apart": KEEP_APART}),
        )
        .await;
    pace.spent(&asked);
    let probability = compose(&asked);
    if let Some(known) = probability {
        remember(memo_key(&groups[a].0, &groups[b].0), known);
    }
    let merge = probability.is_some_and(|p| p >= MERGE_AT) && decider.applies();
    decider
        .mark(
            &asked,
            asked.composed(json!({"score": probability.unwrap_or(1.0)})),
            applied_for(probability, merge),
        )
        .await;
    merge
}

pub(crate) async fn merge<'a>(decider: &Decider, groups: Groups<'a>, repeats: usize) -> Groups<'a> {
    let listed: Vec<(Key, Vec<&'a LogLine>)> = groups.into_iter().collect();
    let mut pace = Pace::new(MAX_ASKS);
    let mut links = Vec::new();
    for pair in candidate_pairs(&listed, repeats) {
        if let Some(known) = remembered(decider, &listed[pair.0].0, &listed[pair.1].0).await {
            if known >= MERGE_AT && decider.applies() {
                links.push(pair);
            }
            continue;
        }
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
