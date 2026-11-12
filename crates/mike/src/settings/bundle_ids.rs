//! The evaluation bundle each judge and steerer runs with, as the settings
//! overlay carries it (eval::bundles publishes and resolves them).

use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct BundleIds {
    pub turn_judge: Option<i64>,
    pub session_judge: Option<i64>,
    pub steerers: BTreeMap<i64, i64>,
}

impl BundleIds {
    pub fn steerer(&self, steerer_id: i64) -> Option<i64> {
        self.steerers.get(&steerer_id).copied()
    }
}
