use std::sync::Arc;

use super::SettingsOverlay;
use super::local_llm::{LOCAL_KEY_ENV_KEY, LOCAL_URL_KEY, SIDE_CALL_MODELS_KEY};
use crate::llm::provider::{PROVIDER_KEY, slot_for_key};

pub fn apply_provider_row(
    overlay: &mut SettingsOverlay,
    key: &str,
    value: &serde_json::Value,
) -> bool {
    match key {
        LOCAL_URL_KEY => {
            super::overlay_apply::set_string(&mut overlay.llm_local_url, key, value);
            return true;
        }
        LOCAL_KEY_ENV_KEY => {
            super::overlay_apply::set_string(&mut overlay.llm_local_api_key_env, key, value);
            return true;
        }
        SIDE_CALL_MODELS_KEY => {
            super::overlay_apply::set_string_list(&mut overlay.side_call_models, key, value);
            return true;
        }
        _ => {}
    }
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
    fn stored_local_rows_set_the_endpoint_and_default_leaves_it_unconfigured() {
        let untouched = applied(&[]);
        assert_eq!(untouched.llm_local_url, "");
        assert_eq!(untouched.llm_local_api_key_env, "MIKE_LOCAL_LLM_API_KEY");
        let overlay = applied(&[
            ("llm.local.url", json!("http://mac.local:8080/v1")),
            ("llm.local.api_key_env", json!("MAC_LLM_KEY")),
        ]);
        assert_eq!(overlay.llm_local_url, "http://mac.local:8080/v1");
        assert_eq!(overlay.llm_local_api_key_env, "MAC_LLM_KEY");
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
