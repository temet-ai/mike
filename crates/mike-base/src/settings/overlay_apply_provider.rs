use std::sync::Arc;

use super::SettingsOverlay;
use crate::llm::provider::{PROVIDER_KEY, slot_for_key};

pub fn apply_provider_row(
    overlay: &mut SettingsOverlay,
    key: &str,
    value: &serde_json::Value,
) -> bool {
    let is_provider = key == PROVIDER_KEY;
    if !is_provider && slot_for_key(key).is_none() {
        return false;
    }
    let text = value.as_str().unwrap_or_default();
    let policy = Arc::make_mut(&mut overlay.llm_provider);
    let accepted = if is_provider {
        policy.set_provider(text)
    } else {
        policy.set_model(key, text)
    };
    if !accepted {
        tracing::warn!(
            key,
            "settings: {key} holds {value}, which is not valid; keeping the default"
        );
    }
    true
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::llm::provider::{LlmProvider, ModelTier};

    fn applied(rows: &[(&str, serde_json::Value)]) -> SettingsOverlay {
        super::super::apply_db_rows_to_overlay(
            rows.iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect(),
        )
    }

    #[test]
    fn no_stored_rows_leave_the_provider_per_role() {
        let overlay = applied(&[]);
        assert_eq!(overlay.llm_provider.provider(), LlmProvider::PerRole);
        assert_eq!(overlay.llm_provider.model_for(ModelTier::Fast), None);
    }

    #[test]
    fn stored_provider_row_selects_that_providers_tier_models() {
        let overlay = applied(&[("llm.provider", json!("claude"))]);
        assert_eq!(overlay.llm_provider.provider(), LlmProvider::Claude);
        assert_eq!(
            overlay.llm_provider.model_for(ModelTier::Fast),
            Some("claude/haiku")
        );
        assert_eq!(
            overlay.llm_provider.model_for(ModelTier::Strong),
            Some("claude/opus")
        );
    }

    #[test]
    fn stored_tier_row_replaces_that_tier_only() {
        let overlay = applied(&[
            ("llm.provider", json!("codex")),
            ("llm.provider_models.codex.fast", json!("codex/gpt-6-astra")),
        ]);
        assert_eq!(
            overlay.llm_provider.model_for(ModelTier::Fast),
            Some("codex/gpt-6-astra")
        );
        assert_eq!(
            overlay.llm_provider.model_for(ModelTier::Standard),
            Some("codex/gpt-6-sol")
        );
    }

    #[test]
    fn unknown_provider_value_keeps_per_role() {
        let overlay = applied(&[("llm.provider", json!("bedrock"))]);
        assert_eq!(overlay.llm_provider.provider(), LlmProvider::PerRole);
    }

    #[test]
    fn blank_tier_model_keeps_the_default() {
        let overlay = applied(&[
            ("llm.provider", json!("claude")),
            ("llm.provider_models.claude.fast", json!("  ")),
        ]);
        assert_eq!(
            overlay.llm_provider.model_for(ModelTier::Fast),
            Some("claude/haiku")
        );
    }
}
