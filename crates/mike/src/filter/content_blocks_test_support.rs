use super::{FilterAction, FilterConfig, FilterKind, FilterOutcome, FilterRule, RuleSource};
use chrono::Utc;
use serde_json::Value;

pub(super) fn rule_with_id(
    numeric_id: i64,
    kind: FilterKind,
    value: &str,
    action: FilterAction,
    text: Option<&str>,
) -> FilterRule {
    FilterRule {
        id: numeric_id,
        uuid: uuid::Uuid::new_v4(),
        kind,
        value: value.to_string(),
        enabled: true,
        source: RuleSource::Toml,
        updated_at: Utc::now(),
        sort_order: 0,
        action,
        text: text.map(|s| s.to_string()),
        trigger_token: None,
        inject_text: None,
        project_path: None,
        style_override: false,
        title: None,
    }
}

pub(super) fn rule(
    id: &str,
    kind: FilterKind,
    value: &str,
    action: FilterAction,
    text: Option<&str>,
) -> FilterRule {
    let numeric_id: i64 = id.bytes().map(|b| b as i64).sum();
    rule_with_id(numeric_id, kind, value, action, text)
}

pub(super) fn cfg(rules: Vec<FilterRule>) -> FilterConfig {
    FilterConfig { rules }
}

pub(super) fn body_bytes(v: Value) -> anyhow::Result<Vec<u8>> {
    Ok(serde_json::to_vec(&v)?)
}

pub(super) fn rewrote(out: FilterOutcome) -> anyhow::Result<(Value, Vec<i64>)> {
    match out {
        FilterOutcome::Rewrote {
            new_body,
            firing_rule_ids,
            ..
        } => Ok((serde_json::from_slice(&new_body)?, firing_rule_ids)),
        FilterOutcome::Unchanged => Err(anyhow::anyhow!("expected Rewrote, got Unchanged")),
    }
}
