use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::bootstrap::stratified_ci;
use super::metrics::{
    DELIVERY_CLASSES, REACTION_CLASSES, WeightedItem, agreement, class_counts, confusion, kappa,
    weighted_rates,
};

#[derive(Debug, Clone, PartialEq)]
pub struct ScoreItem {
    pub turn_id: i64,
    pub gold_reaction: String,
    pub gold_delivery: Option<String>,
    pub pred_reaction: Option<String>,
    pub pred_delivery: Option<String>,
    pub stratum: String,
    pub inclusion_prob: Option<f64>,
}

impl ScoreItem {
    pub fn reaction_pair(&self, r3: bool) -> Option<(&str, &str)> {
        let pred = self.pred_reaction.as_deref()?;
        if r3 {
            let gold = if self.gold_reaction == "corrected" {
                "corrected"
            } else {
                "other"
            };
            let pred = if pred == "corrected" {
                "corrected"
            } else {
                "other"
            };
            return Some((gold, pred));
        }
        Some((self.gold_reaction.as_str(), pred))
    }

    pub fn delivery_pair(&self) -> Option<(&str, &str)> {
        Some((
            self.gold_delivery.as_deref()?,
            self.pred_delivery.as_deref()?,
        ))
    }
}

fn per_class(pairs: &[(&str, &str)], classes: &[&str]) -> Value {
    let map: BTreeMap<&str, Value> = classes
        .iter()
        .map(|c| {
            (
                *c,
                serde_json::to_value(class_counts(pairs, c)).unwrap_or(Value::Null),
            )
        })
        .collect();
    json!(map)
}

fn axis_block(pairs: &[(&str, &str)], classes: &[&str]) -> Value {
    json!({
        "n": pairs.len(),
        "agreement": agreement(pairs),
        "kappa": kappa(pairs),
        "per_class": per_class(pairs, classes),
        "confusion": confusion(pairs),
    })
}

fn corrected_f1(sample: &[&&ScoreItem], r3: bool) -> Option<f64> {
    let pairs: Vec<(&str, &str)> = sample.iter().filter_map(|i| i.reaction_pair(r3)).collect();
    class_counts(&pairs, "corrected").f1
}

fn corrected_block(items: &[&ScoreItem], r3: bool, seed: u64) -> Value {
    let pairs: Vec<(&str, &str)> = items.iter().filter_map(|i| i.reaction_pair(r3)).collect();
    let counts = class_counts(&pairs, "corrected");
    let ci = stratified_ci(items, |i| i.stratum.as_str(), seed, |s| corrected_f1(s, r3));
    json!({
        "precision": counts.precision,
        "recall": counts.recall,
        "f1": counts.f1,
        "tp": counts.tp,
        "fp": counts.fp,
        "fn": counts.false_negatives,
        "f1_ci": ci,
    })
}

fn weighted_block(items: &[&ScoreItem]) -> Value {
    let weighted: Vec<WeightedItem> = items
        .iter()
        .filter_map(|i| {
            let pred = i.pred_reaction.as_deref()?;
            Some(WeightedItem {
                predicted: pred == "corrected",
                positive: i.gold_reaction == "corrected",
                inclusion_prob: i.inclusion_prob,
            })
        })
        .collect();
    serde_json::to_value(weighted_rates(&weighted)).unwrap_or(Value::Null)
}

pub fn score_run(items: &[ScoreItem], r3: bool, seed: u64) -> Value {
    let judged: Vec<&ScoreItem> = items.iter().filter(|i| i.pred_reaction.is_some()).collect();
    let reaction_pairs: Vec<(&str, &str)> =
        judged.iter().filter_map(|i| i.reaction_pair(r3)).collect();
    let delivery_pairs: Vec<(&str, &str)> =
        judged.iter().filter_map(|i| i.delivery_pair()).collect();
    let mut by_stratum: BTreeMap<&str, usize> = BTreeMap::new();
    for item in items {
        *by_stratum.entry(item.stratum.as_str()).or_default() += 1;
    }
    let reaction_classes: Vec<&str> = if r3 {
        vec!["corrected", "other"]
    } else {
        REACTION_CLASSES.to_vec()
    };
    json!({
        "n": judged.len(),
        "n_by_stratum": by_stratum,
        "reaction": axis_block(&reaction_pairs, &reaction_classes),
        "delivery": if r3 { Value::Null } else { axis_block(&delivery_pairs, &DELIVERY_CLASSES) },
        "corrected": corrected_block(&judged, r3, seed),
        "weighted": weighted_block(&judged),
        "r3_binary": r3,
    })
}
