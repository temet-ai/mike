use std::time::Duration;

use crate::llm::{LlmRole, OpenRouterClient};
use crate::state::AppState;

pub(crate) struct SteererClient {
    pub(crate) llm: OpenRouterClient,
    pub(crate) model_used: String,
}

pub(crate) fn steerer_client(state: &AppState, timeout: Duration) -> Result<SteererClient, String> {
    let config = &state.config.steerer;
    let overlay = state.settings_overlay.load();
    let model = overlay.live_steerer_model(&config.model);
    let base = OpenRouterClient::new(
        String::new(),
        model,
        Some(config.openrouter_base_url.clone()),
        Some(timeout),
    )
    .map_err(|e| format!("failed to build LLM client: {e}"))?
    .with_attribution(LlmRole::Steerer, state.llm_usage_sink.as_ref())
    .with_live_settings(crate::settings::steerer_live_source(
        &state.settings_overlay,
    ));
    match crate::llm::key::effective_client(&base, &state.steerer_openrouter_key) {
        Some(llm) => Ok(SteererClient {
            model_used: llm.routed_model(),
            llm,
        }),
        None => Err(crate::llm::openrouter::missing_key_error(
            &base.routed_model(),
            true,
            overlay.llm_provider.provider(),
        )
        .to_string()),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn missing_key_message_names_the_model_and_the_provider() {
        let message = crate::llm::openrouter::missing_key_error(
            "google/gemini-3.8-flash",
            true,
            crate::llm::provider::LlmProvider::PerRole,
        )
        .to_string();
        assert!(message.contains("google/gemini-3.8-flash"), "{message}");
        assert!(message.contains("OpenRouter"), "{message}");
    }
}
