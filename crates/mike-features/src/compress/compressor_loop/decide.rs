use std::collections::HashMap;

use serde_json::Value;

use super::stats::median;
use super::types::{Decision, FILE_SHARE, MIN_SESSIONS_PER_ARM, P_LIMIT, WIDE_SHARE};

#[derive(Debug, Clone, Default)]
pub(super) struct Settings(pub(super) HashMap<String, Value>);

impl Settings {
    pub(super) fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    fn number(&self, key: &str) -> f64 {
        self.get(key).and_then(Value::as_f64).unwrap_or(0.0)
    }
}

pub(super) struct Costs<'a> {
    pub(super) forward: &'a [f64],
    pub(super) control: &'a [f64],
    pub(super) p: f64,
}

impl Costs<'_> {
    fn decisive(&self) -> bool {
        self.forward.len() >= MIN_SESSIONS_PER_ARM
            && self.control.len() >= MIN_SESSIONS_PER_ARM
            && self.p < P_LIMIT
    }

    fn forward_cheaper(&self) -> bool {
        median(self.forward) < median(self.control)
    }
}

pub(super) fn signed_percent(share: f64) -> String {
    format!("{:+.1}%", share * 100.0)
}

pub(super) fn decide(costs: &Costs, settings: &Settings, qualified: bool) -> Vec<Decision> {
    let share = settings.number(FILE_SHARE);
    if !qualified || !costs.decisive() || !costs.forward_cheaper() || share >= WIDE_SHARE {
        return Vec::new();
    }
    vec![Decision {
        action: "widen",
        key: FILE_SHARE,
        value: Value::from(WIDE_SHARE as i64),
        why: format!("forward arm costs less per turn (p={:.3})", costs.p),
    }]
}
