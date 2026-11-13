pub mod components;
pub mod fingerprint;
pub mod resolver;

#[cfg(test)]
pub mod resolver_tests;
#[cfg(test)]
mod tests;

use serde::Serialize;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

pub use resolver::{BundlePublisher, BundleResolver, BundleStaticInputs};

pub const COMPONENTS_VERSION: i64 = 1;

pub use crate::settings::bundle_ids::BundleIds;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleRole {
    TurnJudge,
    SessionJudge,
    Steerer,
}

impl BundleRole {
    pub fn code(self) -> &'static str {
        match self {
            BundleRole::TurnJudge => "turn_judge",
            BundleRole::SessionJudge => "session_judge",
            BundleRole::Steerer => "steerer",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BundleCause {
    pub trigger: &'static str,
    pub change_log_ids: Vec<i64>,
    pub keys: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollup_id: Option<i64>,
}

impl BundleCause {
    fn new(trigger: &'static str) -> Self {
        Self {
            trigger,
            change_log_ids: Vec::new(),
            keys: Vec::new(),
            rollup_id: None,
        }
    }

    pub fn startup() -> Self {
        Self::new("startup")
    }

    pub fn retry() -> Self {
        Self::new("retry")
    }

    pub fn setting(key: &str, change_log_ids: Vec<i64>) -> Self {
        Self {
            change_log_ids,
            keys: vec![key.to_owned()],
            ..Self::new("setting")
        }
    }

    pub fn steerer(steerer_id: i64, change_log_ids: Vec<i64>) -> Self {
        Self {
            change_log_ids,
            keys: vec![steerer_id.to_string()],
            ..Self::new("steerer")
        }
    }

    pub fn composition(composition_key: &str) -> Self {
        Self {
            keys: vec![composition_key.to_owned()],
            ..Self::new("composition")
        }
    }

    pub fn calibration(rollup_id: i64, change_log_ids: Vec<i64>) -> Self {
        Self {
            change_log_ids,
            rollup_id: Some(rollup_id),
            ..Self::new("calibration")
        }
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or_else(|_| json!({"trigger": self.trigger}))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BundleCandidate {
    pub role: BundleRole,
    pub subject_key: String,
    pub components: Value,
    pub content_sha256: String,
}

impl BundleCandidate {
    pub fn new(role: BundleRole, subject_key: String, components: Value) -> Self {
        let content_sha256 = content_sha256(&components);
        Self {
            role,
            subject_key,
            components,
            content_sha256,
        }
    }
}

pub fn canonical_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut sorted = Map::new();
            for key in keys {
                if let Some(inner) = map.get(key) {
                    sorted.insert(key.clone(), canonical_json(inner));
                }
            }
            Value::Object(sorted)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical_json).collect()),
        other => other.clone(),
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn content_sha256(components: &Value) -> String {
    let canonical = canonical_json(components).to_string();
    sha256_hex(canonical.as_bytes())
}
