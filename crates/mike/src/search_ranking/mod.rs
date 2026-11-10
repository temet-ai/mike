use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::store::persistence::project_knowledge::SemanticCut;

pub(crate) mod identifiers;
mod outside_project;
pub(crate) mod tuning;

pub(crate) use outside_project::{is_outside_project, mark_outside_project, weigh_outside_project};

#[cfg(test)]
mod identifiers_tests;
#[cfg(test)]
mod machine_order_tests;
#[cfg(test)]
mod ranking_tests;
#[cfg(test)]
mod tuning_tests;

pub(crate) const KEY_LEXICAL_WEIGHT: &str = "search.ranking.lexical_weight";
pub(crate) const KEY_SEMANTIC_WEIGHT: &str = "search.ranking.semantic_weight";
pub(crate) const KEY_SEMANTIC_FLOOR: &str = "search.ranking.semantic_floor";
pub(crate) const KEY_SEMANTIC_WINDOW: &str = "search.ranking.semantic_window";
pub(crate) const KEY_KIND_WEIGHTS: &str = "search.ranking.kind_weights";

pub(crate) const RANKING_KEYS: [&str; 5] = [
    KEY_LEXICAL_WEIGHT,
    KEY_SEMANTIC_WEIGHT,
    KEY_SEMANTIC_FLOOR,
    KEY_SEMANTIC_WINDOW,
    KEY_KIND_WEIGHTS,
];

pub(crate) const WEIGHT_MIN: f64 = 0.0;
pub(crate) const WEIGHT_MAX: f64 = 5.0;
pub(crate) const FLOOR_MIN: f64 = 0.0;
pub(crate) const FLOOR_MAX: f64 = 1.0;
pub(crate) const WINDOW_MIN: f64 = 0.0;
pub(crate) const WINDOW_MAX: f64 = 0.2;

const DEFAULT_WEIGHT: f64 = 1.0;
const RANK_OFFSET: f64 = 60.0;
const SEAT_KIND: &str = "seat";
const SEAT_BASE_WEIGHT: f64 = 0.5;
pub(crate) const SEAT_MAX_WEIGHT: f64 = 0.9;
pub(crate) const OUTSIDE_PROJECT_SCORE_MARGIN: f64 = 3.0;

pub(crate) fn base_kind_weight(kind: &str) -> f64 {
    if kind == SEAT_KIND {
        SEAT_BASE_WEIGHT
    } else {
        DEFAULT_WEIGHT
    }
}

pub(crate) fn kind_weight_range(kind: &str) -> (f64, f64) {
    if kind == SEAT_KIND {
        (WEIGHT_MIN, SEAT_MAX_WEIGHT)
    } else {
        (WEIGHT_MIN, WEIGHT_MAX)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RankingConfig {
    pub(crate) lexical_weight: f64,
    pub(crate) semantic_weight: f64,
    pub(crate) semantic_floor: Option<f64>,
    pub(crate) semantic_window: Option<f64>,
    pub(crate) kind_weights: BTreeMap<String, f64>,
}

impl Default for RankingConfig {
    fn default() -> Self {
        Self {
            lexical_weight: DEFAULT_WEIGHT,
            semantic_weight: DEFAULT_WEIGHT,
            semantic_floor: None,
            semantic_window: None,
            kind_weights: BTreeMap::new(),
        }
    }
}

impl RankingConfig {
    pub(crate) fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub(crate) fn semantic_cut(&self) -> SemanticCut {
        SemanticCut {
            floor: self.semantic_floor,
            window: self.semantic_window,
        }
    }

    pub(crate) fn kind_weight(&self, kind: &str) -> f64 {
        self.kind_weights
            .get(kind)
            .copied()
            .unwrap_or_else(|| base_kind_weight(kind))
    }

    pub(crate) fn value(&self, key: &str) -> Option<Value> {
        match key {
            KEY_LEXICAL_WEIGHT => Some(json!(self.lexical_weight)),
            KEY_SEMANTIC_WEIGHT => Some(json!(self.semantic_weight)),
            KEY_SEMANTIC_FLOOR => Some(json!(self.semantic_floor)),
            KEY_SEMANTIC_WINDOW => Some(json!(self.semantic_window)),
            KEY_KIND_WEIGHTS => Some(self.kind_weights_value()),
            _ => None,
        }
    }

    fn kind_weights_value(&self) -> Value {
        Value::Object(
            self.kind_weights
                .iter()
                .map(|(kind, weight)| (kind.clone(), json!(weight)))
                .collect::<Map<String, Value>>(),
        )
    }

    pub(crate) fn apply(&mut self, key: &str, value: &Value) -> bool {
        match key {
            KEY_LEXICAL_WEIGHT => assign(&mut self.lexical_weight, value, WEIGHT_MIN, WEIGHT_MAX),
            KEY_SEMANTIC_WEIGHT => assign(&mut self.semantic_weight, value, WEIGHT_MIN, WEIGHT_MAX),
            KEY_SEMANTIC_FLOOR => {
                assign_optional(&mut self.semantic_floor, value, FLOOR_MIN, FLOOR_MAX)
            }
            KEY_SEMANTIC_WINDOW => {
                assign_optional(&mut self.semantic_window, value, WINDOW_MIN, WINDOW_MAX)
            }
            KEY_KIND_WEIGHTS => self.assign_kind_weights(value),
            _ => false,
        }
    }

    fn assign_kind_weights(&mut self, value: &Value) -> bool {
        let Some(object) = value.as_object() else {
            return false;
        };
        let mut weights = BTreeMap::new();
        for (kind, weight) in object {
            let Some(weight) = weight.as_f64().filter(|w| w.is_finite()) else {
                return false;
            };
            let (min, max) = kind_weight_range(kind);
            let weight = weight.clamp(min, max);
            if weight != base_kind_weight(kind) {
                weights.insert(kind.clone(), weight);
            }
        }
        self.kind_weights = weights;
        true
    }
}

fn assign(slot: &mut f64, value: &Value, min: f64, max: f64) -> bool {
    match value.as_f64().filter(|v| v.is_finite()) {
        Some(v) => {
            *slot = v.clamp(min, max);
            true
        }
        None => false,
    }
}

fn assign_optional(slot: &mut Option<f64>, value: &Value, min: f64, max: f64) -> bool {
    if value.is_null() {
        *slot = None;
        return true;
    }
    let mut current = slot.unwrap_or(min);
    let assigned = assign(&mut current, value, min, max);
    if assigned {
        *slot = Some(current);
    }
    assigned
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Candidate {
    pub(crate) reference: String,
    pub(crate) kind: String,
    pub(crate) lexical: f64,
    pub(crate) semantic: f64,
    pub(crate) ranked_lower: bool,
}

impl Candidate {
    pub(crate) fn from_row(row: &Value) -> Self {
        Self {
            reference: text(row, "reference"),
            kind: text(row, "source_kind"),
            lexical: number(row, "lexical_rank"),
            semantic: number(row, "semantic_rank"),
            ranked_lower: row["provenance"]["ranked_lower"].as_bool() == Some(true),
        }
    }
}

impl RankingConfig {
    pub(crate) fn weight(&self, candidate: &Candidate) -> f64 {
        if candidate.ranked_lower && candidate.kind != SEAT_KIND {
            SEAT_BASE_WEIGHT
        } else {
            self.kind_weight(&candidate.kind)
        }
    }
}

pub(crate) fn order_by_position(candidates: &[Candidate], config: &RankingConfig) -> Vec<usize> {
    let scores: Vec<f64> = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            let weight = if candidate.ranked_lower {
                config.weight(candidate)
            } else {
                DEFAULT_WEIGHT
            };
            weight / (RANK_OFFSET + (index + 1) as f64)
        })
        .collect();
    let mut indices: Vec<usize> = (0..candidates.len()).collect();
    indices.sort_by(|a, b| scores[*b].total_cmp(&scores[*a]).then(a.cmp(b)));
    indices
}

fn text(row: &Value, field: &str) -> String {
    row[field].as_str().unwrap_or_default().to_owned()
}

fn number(row: &Value, field: &str) -> f64 {
    row[field].as_f64().unwrap_or(0.0)
}

pub(crate) fn identifier_hits(row: &Value) -> u64 {
    row["identifier_hits"].as_u64().unwrap_or(0)
}

pub(crate) fn lift_identifier_rows<T>(mut rows: Vec<(Value, T)>) -> Vec<(Value, T)> {
    rows.sort_by_key(|(row, _)| std::cmp::Reverse(identifier_hits(row)));
    rows
}

pub(crate) fn order(candidates: &[Candidate], config: &RankingConfig) -> Vec<usize> {
    let best = candidates.iter().map(|c| c.semantic).fold(0.0, f64::max);
    let floor = config.semantic_floor.unwrap_or(f64::NEG_INFINITY);
    let threshold = floor.max(
        config
            .semantic_window
            .map_or(f64::NEG_INFINITY, |w| best - w),
    );
    let lexical = positions(candidates, |c| (c.lexical > 0.0).then_some(c.lexical));
    let semantic = positions(candidates, |c| {
        (c.semantic > 0.0 && c.semantic >= threshold).then_some(c.semantic)
    });
    let fused: Vec<f64> = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            let lexical = contribution(config.lexical_weight, lexical[index]);
            let semantic = contribution(config.semantic_weight, semantic[index]);
            (lexical + semantic) * config.weight(candidate)
        })
        .collect();
    let mut indices: Vec<usize> = (0..candidates.len()).collect();
    indices.sort_by(|a, b| fused[*b].total_cmp(&fused[*a]).then(a.cmp(b)));
    indices
}

fn contribution(weight: f64, position: Option<usize>) -> f64 {
    position.map_or(0.0, |p| weight / (RANK_OFFSET + p as f64))
}

fn positions(
    candidates: &[Candidate],
    score: impl Fn(&Candidate) -> Option<f64>,
) -> Vec<Option<usize>> {
    let mut counted: Vec<(usize, f64)> = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| score(candidate).map(|s| (index, s)))
        .collect();
    counted.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut positions = vec![None; candidates.len()];
    for (position, (index, _)) in counted.into_iter().enumerate() {
        positions[index] = Some(position + 1);
    }
    positions
}
