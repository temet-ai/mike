use crate::llm::{LlmRole, LlmUsageSink};

use super::super::schema::ProxyConfig;

pub(super) fn seed_openrouter_key_slot(config: &ProxyConfig) -> crate::llm::key::OpenRouterKeySlot {
    std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(
        crate::llm::key::seed_from_env(config.steerer.openrouter_api_key.as_ref()),
    ))
}

pub(super) fn seed_anthropic_key_slot() -> crate::llm::key::AnthropicKeySlot {
    std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(
        crate::llm::key::seed_anthropic_from_env(),
    ))
}

pub(super) fn build_judge_llm(
    steerer_config: &crate::steerer::config::SteererConfig,
    usage_sink: Option<&std::sync::Arc<dyn LlmUsageSink>>,
) -> Option<crate::llm::OpenRouterClient> {
    let judge_cfg = steerer_config.judge.clone();
    let model = resolve_role_model(judge_cfg.model.as_deref(), &steerer_config.model);

    match build_role_client(steerer_config, model) {
        Ok(llm) => Some(llm.with_attribution(LlmRole::JudgeWorker, usage_sink)),
        Err(e) => {
            log_judge_client_build_failed(&e);
            None
        }
    }
}

fn log_judge_client_build_failed(e: &crate::llm::LlmError) {
    tracing::warn!(
        error = %e,
        "judge LLM: failed to build client — turn labeling disabled"
    );
}

fn resolve_role_model(role_model: Option<&str>, steerer_model: &str) -> String {
    role_model
        .filter(|m| !m.is_empty())
        .unwrap_or(steerer_model)
        .to_owned()
}

fn build_role_client(
    steerer_config: &crate::steerer::config::SteererConfig,
    model: String,
) -> Result<crate::llm::OpenRouterClient, crate::llm::LlmError> {
    crate::llm::OpenRouterClient::new(
        String::new(),
        model,
        Some(steerer_config.openrouter_base_url.clone()),
        Some(std::time::Duration::from_secs(
            steerer_config.request_timeout_secs,
        )),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::steerer::config::SteererConfig;

    #[test]
    fn build_judge_llm_builds_a_client_even_when_boot_judge_flag_is_false() {
        let config = SteererConfig::default();
        assert!(!config.judge.enabled);

        let llm = build_judge_llm(&config, None);

        assert!(llm.is_some());
    }

    #[test]
    fn build_judge_llm_uses_the_judge_model_override_when_set() {
        let mut config = SteererConfig::default();
        config.judge.model = Some("google/gemini-3.8-flash".to_owned());

        let llm = build_judge_llm(&config, None);

        assert_eq!(
            llm.map(|c| c.model().to_owned()).as_deref(),
            Some("google/gemini-3.8-flash")
        );
    }

    #[test]
    fn build_judge_llm_falls_back_to_the_steerer_model() {
        let mut config = SteererConfig::default();
        config.judge.model = None;

        let llm = build_judge_llm(&config, None);

        assert_eq!(
            llm.map(|c| c.model().to_owned()).as_deref(),
            Some(config.model.as_str())
        );
    }
}
