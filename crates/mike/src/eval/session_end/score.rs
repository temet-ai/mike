use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{Value, json};

use crate::eval::score::bootstrap::bootstrap_ci;
use crate::eval::score::metrics::{class_counts, confusion, kappa};

pub const END_LABELS: [&str; 5] = [
    "done",
    "closed_in_frustration",
    "cut_off_neutral",
    "fleet_seat",
    "unclear",
];

const FRUSTRATION: &str = "closed_in_frustration";
const WILSON_Z: f64 = 1.96;

#[derive(Debug, Clone, PartialEq)]
pub struct EndItem {
    pub gold: String,
    pub llm: Option<String>,
    pub rule_fired: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Variant {
    ModelOnly,
    FleetSeat,
    Done,
    Both,
}

impl Variant {
    pub const ALL: [Self; 4] = [Self::ModelOnly, Self::FleetSeat, Self::Done, Self::Both];

    pub fn key(self) -> &'static str {
        match self {
            Self::ModelOnly => "model_only",
            Self::FleetSeat => "fleet_seat_v1",
            Self::Done => "done_v1",
            Self::Both => "both",
        }
    }

    pub fn applies(self, rule: &str) -> bool {
        match self {
            Self::ModelOnly => false,
            Self::FleetSeat => rule == "fleet-seat-v1",
            Self::Done => rule == "done-v1",
            Self::Both => rule == "fleet-seat-v1" || rule == "done-v1",
        }
    }

    pub fn for_rules(rules: &[&str]) -> Self {
        let fleet = rules.contains(&"fleet-seat-v1");
        let done = rules.contains(&"done-v1");
        match (fleet, done) {
            (true, true) => Self::Both,
            (true, false) => Self::FleetSeat,
            (false, true) => Self::Done,
            (false, false) => Self::ModelOnly,
        }
    }
}

pub fn rule_label(rule: &str) -> Option<&'static str> {
    match rule {
        "fleet-seat-v1" => Some("fleet_seat"),
        "done-v1" => Some("done"),
        _ => None,
    }
}

pub fn effective_label(item: &EndItem, variant: Variant) -> Option<&str> {
    let ruled = item
        .rule_fired
        .as_deref()
        .filter(|r| variant.applies(r))
        .and_then(rule_label);
    ruled.or(item.llm.as_deref())
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Rate {
    pub k: i64,
    pub n: i64,
    pub value: Option<f64>,
    pub low: Option<f64>,
    pub high: Option<f64>,
}

pub fn wilson_rate(k: i64, n: i64) -> Rate {
    if n <= 0 {
        return Rate {
            k,
            n,
            value: None,
            low: None,
            high: None,
        };
    }
    let nf = n as f64;
    let p = k as f64 / nf;
    let z2 = WILSON_Z * WILSON_Z;
    let centre = (p + z2 / (2.0 * nf)) / (1.0 + z2 / nf);
    let half = WILSON_Z * ((p * (1.0 - p) / nf) + z2 / (4.0 * nf * nf)).sqrt() / (1.0 + z2 / nf);
    Rate {
        k,
        n,
        value: Some(p),
        low: Some((centre - half).max(0.0)),
        high: Some((centre + half).min(1.0)),
    }
}

pub fn pairs(items: &[EndItem], variant: Variant) -> Vec<(&str, &str)> {
    items
        .iter()
        .filter_map(|i| effective_label(i, variant).map(|p| (i.gold.as_str(), p)))
        .collect()
}

pub fn frustration_rates(pairs: &[(&str, &str)]) -> (Rate, Rate) {
    let c = class_counts(pairs, FRUSTRATION);
    (
        wilson_rate(i64::from(c.tp), i64::from(c.tp + c.fp)),
        wilson_rate(i64::from(c.tp), i64::from(c.tp + c.false_negatives)),
    )
}

pub fn rule_precision(items: &[EndItem], rule: &str) -> (Rate, Rate) {
    let Some(label) = rule_label(rule) else {
        return (wilson_rate(0, 0), wilson_rate(0, 0));
    };
    let fires: Vec<&EndItem> = items
        .iter()
        .filter(|i| i.rule_fired.as_deref() == Some(rule))
        .collect();
    let n = fires.len() as i64;
    let right = fires.iter().filter(|i| i.gold == label).count() as i64;
    let frustrated = fires.iter().filter(|i| i.gold == FRUSTRATION).count() as i64;
    (wilson_rate(right, n), wilson_rate(frustrated, n))
}

fn per_label(pairs: &[(&str, &str)]) -> BTreeMap<&'static str, Value> {
    END_LABELS
        .iter()
        .map(|label| {
            let c = class_counts(pairs, label);
            (*label, json!({"precision": c.precision, "recall": c.recall, "tp": c.tp, "fp": c.fp, "fn": c.false_negatives}))
        })
        .collect()
}

pub fn variant_kappa(items: &[EndItem], variant: Variant) -> Option<f64> {
    kappa(&pairs(items, variant))
}

pub fn score_variant(items: &[EndItem], variant: Variant, seed: u64) -> Value {
    let p = pairs(items, variant);
    let ci = bootstrap_ci(items, seed, |sample| {
        let owned: Vec<EndItem> = sample.iter().map(|i| (*i).clone()).collect();
        variant_kappa(&owned, variant)
    });
    let (precision, recall) = frustration_rates(&p);
    json!({
        "n": p.len(),
        "kappa": kappa(&p),
        "kappa_ci": ci.map(|(low, high)| json!({"low": low, "high": high})),
        "confusion": confusion(&p),
        "per_label": per_label(&p),
        "frustration": {"precision": precision, "recall": recall},
    })
}

pub fn score_session_end_run(items: &[EndItem], seed: u64) -> Value {
    let variants: BTreeMap<&str, Value> = Variant::ALL
        .iter()
        .map(|v| (v.key(), score_variant(items, *v, seed)))
        .collect();
    let rules: BTreeMap<&str, Value> = ["fleet-seat-v1", "done-v1"]
        .iter()
        .map(|rule| {
            let (precision, frustration_among_fires) = rule_precision(items, rule);
            (
                *rule,
                json!({"precision": precision, "frustration_among_fires": frustration_among_fires}),
            )
        })
        .collect();
    json!({
        "items": items.len(),
        "unscored": items.iter().filter(|i| effective_label(i, Variant::Both).is_none()).count(),
        "variants": variants,
        "rules": rules,
    })
}
