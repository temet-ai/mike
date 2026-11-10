use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    Candidate, KEY_KIND_WEIGHTS, KEY_LEXICAL_WEIGHT, KEY_SEMANTIC_FLOOR, KEY_SEMANTIC_WEIGHT,
    KEY_SEMANTIC_WINDOW, RANKING_KEYS, RankingConfig, kind_weight_range, order,
};

pub(crate) const TOP_N: usize = 5;
pub(crate) const MIN_GRADED_SEARCHES: usize = 20;
pub(crate) const MIN_GAIN: f64 = 0.01;

const TOLERANCE: f64 = 1e-9;
const WEIGHT_FACTORS: [f64; 4] = [0.5, 0.75, 1.25, 1.5];
const KIND_FACTORS: [f64; 2] = [0.8, 1.25];
const FLOOR_VALUES: [f64; 6] = [0.45, 0.5, 0.55, 0.6, 0.65, 0.7];
const WINDOW_VALUES: [f64; 4] = [0.04, 0.06, 0.08, 0.12];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct GradeCandidate {
    pub(crate) reference: String,
    pub(crate) kind: String,
    pub(crate) lexical: f64,
    pub(crate) semantic: f64,
    pub(crate) rerank_score: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Grade {
    pub(crate) candidates: Vec<GradeCandidate>,
    pub(crate) used: Vec<String>,
}

impl Grade {
    pub(crate) fn from_stored(candidates: Value, used: Vec<String>) -> Option<Self> {
        let candidates = serde_json::from_value(candidates).ok()?;
        Some(Self { candidates, used })
    }

    fn kind_of(&self, reference: &str) -> String {
        match self.candidates.iter().find(|c| c.reference == reference) {
            Some(candidate) => candidate.kind.clone(),
            None => reference.split(':').next().unwrap_or_default().to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Evaluation {
    pub(crate) overall: f64,
    pub(crate) by_kind: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Attempt {
    pub(crate) key: &'static str,
    pub(crate) before: Value,
    pub(crate) after: Value,
    pub(crate) score_before: f64,
    pub(crate) score_after: f64,
    pub(crate) kept: bool,
    pub(crate) reason: String,
}

pub(crate) fn reranked_order(
    order: Vec<usize>,
    scores: &[Option<f64>],
    window: usize,
) -> Vec<usize> {
    let split = window.min(order.len());
    let (head, tail) = order.split_at(split);
    let (mut scored, unscored): (Vec<usize>, Vec<usize>) = head
        .iter()
        .partition(|index| scores.get(**index).copied().flatten().is_some());
    scored.sort_by(|a, b| score_at(scores, *b).total_cmp(&score_at(scores, *a)));
    scored
        .into_iter()
        .chain(unscored)
        .chain(tail.iter().copied())
        .collect()
}

fn score_at(scores: &[Option<f64>], index: usize) -> f64 {
    scores.get(index).copied().flatten().unwrap_or(f64::MIN)
}

pub(crate) fn evaluate(grades: &[Grade], config: &RankingConfig, window: usize) -> Evaluation {
    let mut totals: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    for grade in grades {
        let top = top_references(grade, config, window);
        for used in &grade.used {
            let entry = totals.entry(grade.kind_of(used)).or_default();
            entry.1 += 1;
            entry.0 += u32::from(top.contains(used.as_str()));
        }
    }
    let (hits, total) = totals
        .values()
        .fold((0, 0), |(h, t), (hits, total)| (h + hits, t + total));
    Evaluation {
        overall: share(hits, total),
        by_kind: totals
            .into_iter()
            .map(|(kind, (hits, total))| (kind, share(hits, total)))
            .collect(),
    }
}

fn share(hits: u32, total: u32) -> f64 {
    if total == 0 {
        0.0
    } else {
        f64::from(hits) / f64::from(total)
    }
}

fn top_references<'a>(
    grade: &'a Grade,
    config: &RankingConfig,
    window: usize,
) -> BTreeSet<&'a str> {
    let candidates: Vec<Candidate> = grade
        .candidates
        .iter()
        .map(|c| Candidate {
            reference: c.reference.clone(),
            kind: c.kind.clone(),
            lexical: c.lexical,
            semantic: c.semantic,
            machine: false,
        })
        .collect();
    let scores: Vec<Option<f64>> = grade.candidates.iter().map(|c| c.rerank_score).collect();
    reranked_order(order(&candidates, config), &scores, window)
        .into_iter()
        .take(TOP_N)
        .map(|index| grade.candidates[index].reference.as_str())
        .collect()
}

pub(crate) fn verdict(
    before: &Evaluation,
    after: &Evaluation,
    min_gain: f64,
) -> Result<String, String> {
    let gain = after.overall - before.overall;
    if gain < min_gain - TOLERANCE {
        return Err(format!(
            "the share of used references in the top {TOP_N} moved {gain:+.4}, under the {min_gain} needed"
        ));
    }
    for (kind, was) in &before.by_kind {
        let now = after.by_kind.get(kind).copied().unwrap_or(0.0);
        if now < *was - TOLERANCE {
            return Err(format!(
                "the share for {kind} would drop from {was:.4} to {now:.4}"
            ));
        }
    }
    Ok(format!(
        "the share of used references in the top {TOP_N} rose by {gain:+.4} and no kind dropped"
    ))
}

pub(crate) fn tune(
    grades: &[Grade],
    start: &RankingConfig,
    window: usize,
    min_gain: f64,
) -> Vec<Attempt> {
    let kinds = used_kinds(grades);
    let mut config = start.clone();
    let mut attempts = Vec::new();
    for key in RANKING_KEYS {
        if let Some(attempt) = tune_setting(key, grades, (&config, window, min_gain), &kinds) {
            if attempt.kept {
                config.apply(key, &attempt.after);
            }
            attempts.push(attempt);
        }
    }
    attempts
}

fn used_kinds(grades: &[Grade]) -> BTreeSet<String> {
    grades
        .iter()
        .flat_map(|g| g.used.iter().map(|u| g.kind_of(u)))
        .collect()
}

fn tune_setting(
    key: &'static str,
    grades: &[Grade],
    (config, window, min_gain): (&RankingConfig, usize, f64),
    kinds: &BTreeSet<String>,
) -> Option<Attempt> {
    let before = evaluate(grades, config, window);
    let before_value = config.value(key)?;
    let mut best_kept: Option<(Value, Evaluation, String)> = None;
    let mut best_rejected: Option<(Value, Evaluation, String)> = None;
    for value in candidate_values(key, config, kinds) {
        let mut trial = config.clone();
        trial.apply(key, &value);
        let after = evaluate(grades, &trial, window);
        let (slot, reason) = match verdict(&before, &after, min_gain) {
            Ok(reason) => (&mut best_kept, reason),
            Err(reason) => (&mut best_rejected, reason),
        };
        if slot
            .as_ref()
            .is_none_or(|(_, best, _)| after.overall > best.overall)
        {
            *slot = Some((value, after, reason));
        }
    }
    let kept = best_kept.is_some();
    let (after_value, after, reason) = best_kept.or(best_rejected)?;
    Some(Attempt {
        key,
        before: before_value,
        after: after_value,
        score_before: before.overall,
        score_after: after.overall,
        kept,
        reason,
    })
}

fn candidate_values(key: &str, config: &RankingConfig, kinds: &BTreeSet<String>) -> Vec<Value> {
    let proposals: Vec<Value> = match key {
        KEY_LEXICAL_WEIGHT => scaled(config.lexical_weight),
        KEY_SEMANTIC_WEIGHT => scaled(config.semantic_weight),
        KEY_SEMANTIC_FLOOR => FLOOR_VALUES.iter().map(|v| serde_json::json!(v)).collect(),
        KEY_SEMANTIC_WINDOW => WINDOW_VALUES.iter().map(|v| serde_json::json!(v)).collect(),
        KEY_KIND_WEIGHTS => kind_proposals(config, kinds),
        _ => Vec::new(),
    };
    let mut distinct: Vec<Value> = Vec::new();
    for proposal in proposals {
        let mut trial = config.clone();
        if !trial.apply(key, &proposal) || trial == *config {
            continue;
        }
        let normalised = trial.value(key).unwrap_or(proposal);
        if !distinct.contains(&normalised) {
            distinct.push(normalised);
        }
    }
    distinct
}

fn scaled(current: f64) -> Vec<Value> {
    WEIGHT_FACTORS
        .iter()
        .map(|factor| serde_json::json!(round(current * factor)))
        .collect()
}

fn kind_proposals(config: &RankingConfig, kinds: &BTreeSet<String>) -> Vec<Value> {
    let mut proposals = Vec::new();
    for kind in kinds {
        for factor in KIND_FACTORS {
            let mut weights = config.kind_weights.clone();
            let (min, max) = kind_weight_range(kind);
            let weight = round(config.kind_weight(kind) * factor).clamp(min, max);
            weights.insert(kind.clone(), weight);
            proposals.push(Value::Object(
                weights.into_iter().map(|(k, w)| (k, w.into())).collect(),
            ));
        }
    }
    proposals
}

fn round(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}
