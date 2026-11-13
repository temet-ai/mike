use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::types::{FilterAction, FilterKind, FilterRule, RuleSource};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFilterRule {
    #[serde(default)]
    uuid: Option<String>,
    kind: FilterKind,
    value: String,
    #[serde(default = "default_enabled")]
    enabled: bool,
    #[serde(default)]
    source: Option<RuleSource>,
    #[serde(default)]
    updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    sort_order: i32,
    action: FilterAction,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    trigger_token: Option<String>,
}

fn default_enabled() -> bool {
    true
}

impl<'de> Deserialize<'de> for FilterRule {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;
        let raw = RawFilterRule::deserialize(deserializer)?;
        let uuid = match raw.uuid {
            None => uuid::Uuid::new_v4(),
            Some(raw_uuid) => uuid::Uuid::parse_str(&raw_uuid).map_err(|e| {
                D::Error::custom(format!(
                    "filter rule uuid {raw_uuid:?} (matching value {value:?}) is not a valid UUID: {e}. \
                     Replace with a UUIDv4 (e.g. via `uuidgen` or Rust's `uuid::Uuid::new_v4()`).",
                    value = raw.value,
                ))
            })?,
        };
        let rule = FilterRule {
            id: 0,
            uuid,
            kind: raw.kind,
            value: raw.value,
            enabled: raw.enabled,
            source: raw.source.unwrap_or(RuleSource::Toml),
            updated_at: raw.updated_at.unwrap_or_else(Utc::now),
            sort_order: raw.sort_order,
            action: raw.action,
            text: raw.text,
            trigger_token: raw.trigger_token,
            inject_text: None,
            project_path: None,
            style_override: false,
            title: None,
        };
        rule.validate().map_err(D::Error::custom)?;
        Ok(rule)
    }
}
