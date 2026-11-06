use super::OpenRouterClient;
use crate::llm::LlmError;
use crate::llm::model_ref::{ModelRef, Provider};
use crate::llm::provider::{self, LlmProvider};

impl OpenRouterClient {
    pub(crate) fn routed_model(&self) -> String {
        provider::resolve(self.inner.tier, &self.inner.model)
    }

    pub(crate) fn target(&self) -> ModelRef {
        ModelRef::parse(&self.routed_model())
    }

    pub(crate) fn has_api_key(&self) -> bool {
        !self.inner.api_key.is_empty()
    }

    pub(super) fn wire_model(&self) -> String {
        let own = ModelRef::parse(&self.inner.model);
        match own.provider {
            Provider::OpenRouter => own.model,
            Provider::Claude | Provider::Codex => self.inner.model.clone(),
        }
    }

    pub(super) fn require_api_key(&self) -> Result<(), LlmError> {
        if self.has_api_key() {
            return Ok(());
        }
        Err(missing_key_error(
            &self.inner.model,
            self.inner.tier.is_some(),
            provider::current().provider(),
        ))
    }
}

pub(crate) fn missing_key_error(model: &str, has_tier: bool, policy: LlmProvider) -> LlmError {
    let note = if has_tier || policy == LlmProvider::PerRole {
        String::new()
    } else {
        format!(
            "; llm.provider is {}, which does not move a job that has no tier, so this job keeps the model it was given",
            policy.as_str()
        )
    };
    LlmError::ProviderUnavailable {
        model: model.to_owned(),
        provider: Provider::OpenRouter.display_name(),
        missing: format!(
            "no OpenRouter API key is configured{note}. Set MIKE_STEERER_OPENROUTER_API_KEY or OPENROUTER_API_KEY, or give this job a claude/ or codex/ model"
        ),
    }
}
