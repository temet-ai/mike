use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

pub const REACTION_CLASSES: [&str; 5] = [
    "corrected",
    "redirected",
    "accepted",
    "no_response",
    "unclear",
];
pub const DELIVERY_CLASSES: [&str; 6] = [
    "shipped",
    "delivered",
    "partial",
    "none",
    "question",
    "not_applicable",
];

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ClassCounts {
    pub tp: u32,
    pub fp: u32,
    #[serde(rename = "fn")]
    pub false_negatives: u32,
    pub precision: Option<f64>,
    pub recall: Option<f64>,
    pub f1: Option<f64>,
}

fn ratio(k: u32, n: u32) -> Option<f64> {
    (n > 0).then(|| f64::from(k) / f64::from(n))
}

pub fn f1_of(precision: Option<f64>, recall: Option<f64>) -> Option<f64> {
    match (precision, recall) {
        (Some(p), Some(r)) if p + r > 0.0 => Some(2.0 * p * r / (p + r)),
        _ => None,
    }
}

pub fn class_counts(pairs: &[(&str, &str)], class: &str) -> ClassCounts {
    let tp = pairs
        .iter()
        .filter(|(g, p)| *g == class && *p == class)
        .count();
    let fp = pairs
        .iter()
        .filter(|(g, p)| *g != class && *p == class)
        .count();
    let fne = pairs
        .iter()
        .filter(|(g, p)| *g == class && *p != class)
        .count();
    let (tp, fp, fne) = (
        u32::try_from(tp).unwrap_or(u32::MAX),
        u32::try_from(fp).unwrap_or(u32::MAX),
        u32::try_from(fne).unwrap_or(u32::MAX),
    );
    let precision = ratio(tp, tp + fp);
    let recall = ratio(tp, tp + fne);
    ClassCounts {
        tp,
        fp,
        false_negatives: fne,
        precision,
        recall,
        f1: f1_of(precision, recall),
    }
}

pub fn kappa(pairs: &[(&str, &str)]) -> Option<f64> {
    if pairs.is_empty() {
        return None;
    }
    let n = pairs.len() as f64;
    let po = pairs.iter().filter(|(a, b)| a == b).count() as f64 / n;
    let labels: BTreeSet<&str> = pairs.iter().flat_map(|(a, b)| [*a, *b]).collect();
    let pe: f64 = labels
        .iter()
        .map(|l| {
            let ca = pairs.iter().filter(|(a, _)| a == l).count() as f64;
            let cb = pairs.iter().filter(|(_, b)| b == l).count() as f64;
            ca * cb
        })
        .sum::<f64>()
        / (n * n);
    Some(if pe < 1.0 {
        (po - pe) / (1.0 - pe)
    } else {
        1.0
    })
}

pub fn agreement(pairs: &[(&str, &str)]) -> usize {
    pairs.iter().filter(|(a, b)| a == b).count()
}

pub fn confusion(pairs: &[(&str, &str)]) -> BTreeMap<String, BTreeMap<String, u32>> {
    let mut out: BTreeMap<String, BTreeMap<String, u32>> = BTreeMap::new();
    for (gold, pred) in pairs {
        *out.entry((*gold).to_owned())
            .or_default()
            .entry((*pred).to_owned())
            .or_default() += 1;
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeightedItem {
    pub predicted: bool,
    pub positive: bool,
    pub inclusion_prob: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct WeightedRates {
    pub precision: Option<f64>,
    pub recall: Option<f64>,
    pub excluded_null_weight: usize,
}

pub fn weighted_rates(items: &[WeightedItem]) -> WeightedRates {
    let mut tp = 0.0;
    let mut pred = 0.0;
    let mut pos = 0.0;
    let mut excluded = 0;
    for item in items {
        let Some(p) = item.inclusion_prob.filter(|p| *p > 0.0) else {
            excluded += 1;
            continue;
        };
        let w = 1.0 / p;
        if item.predicted && item.positive {
            tp += w;
        }
        if item.predicted {
            pred += w;
        }
        if item.positive {
            pos += w;
        }
    }
    WeightedRates {
        precision: (pred > 0.0).then(|| tp / pred),
        recall: (pos > 0.0).then(|| tp / pos),
        excluded_null_weight: excluded,
    }
}

pub fn gold_delivery(delivery: &str, irreversible_action: bool) -> &str {
    if delivery == "delivered" && irreversible_action {
        "shipped"
    } else {
        delivery
    }
}
