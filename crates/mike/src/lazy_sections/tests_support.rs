use super::*;

pub(super) fn cfg(rules: Vec<FilterRule>) -> FilterConfig {
    FilterConfig { rules }
}

pub(super) fn rewrote(out: LazySectionsOutcome) -> anyhow::Result<(Value, usize, bool, Vec<i64>)> {
    match out {
        LazySectionsOutcome::Rewrote {
            new_body,
            stripped_bytes,
            injected,
            firing_rule_ids,
            ..
        } => Ok((
            serde_json::from_slice(&new_body)?,
            stripped_bytes,
            injected,
            firing_rule_ids,
        )),
        LazySectionsOutcome::Unchanged => Err(anyhow::anyhow!("expected Rewrote, got Unchanged")),
    }
}
