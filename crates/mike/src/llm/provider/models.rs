use super::{LlmProvider, ModelTier};
use crate::llm::model_ref::{ModelRef, ModelRefError, Provider};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ModelSlot {
    pub(crate) key: &'static str,
    pub(crate) provider: LlmProvider,
    pub(crate) tier: ModelTier,
    pub(crate) default: &'static str,
}

const fn slot(
    key: &'static str,
    provider: LlmProvider,
    tier: ModelTier,
    default: &'static str,
) -> ModelSlot {
    ModelSlot {
        key,
        provider,
        tier,
        default,
    }
}

pub(crate) const SLOTS: [ModelSlot; 9] = [
    slot(
        "llm.provider_models.openrouter.fast",
        LlmProvider::OpenRouter,
        ModelTier::Fast,
        "google/gemini-3.5-flash-lite",
    ),
    slot(
        "llm.provider_models.openrouter.standard",
        LlmProvider::OpenRouter,
        ModelTier::Standard,
        "google/gemini-3.8-flash",
    ),
    slot(
        "llm.provider_models.openrouter.strong",
        LlmProvider::OpenRouter,
        ModelTier::Strong,
        "openai/gpt-5.6-luna",
    ),
    slot(
        "llm.provider_models.claude.fast",
        LlmProvider::Claude,
        ModelTier::Fast,
        "claude/haiku",
    ),
    slot(
        "llm.provider_models.claude.standard",
        LlmProvider::Claude,
        ModelTier::Standard,
        "claude/sonnet",
    ),
    slot(
        "llm.provider_models.claude.strong",
        LlmProvider::Claude,
        ModelTier::Strong,
        "claude/opus",
    ),
    slot(
        "llm.provider_models.codex.fast",
        LlmProvider::Codex,
        ModelTier::Fast,
        "codex/gpt-6-luna",
    ),
    slot(
        "llm.provider_models.codex.standard",
        LlmProvider::Codex,
        ModelTier::Standard,
        "codex/gpt-6-sol",
    ),
    slot(
        "llm.provider_models.codex.strong",
        LlmProvider::Codex,
        ModelTier::Strong,
        "codex/gpt-6.1-sol",
    ),
];

pub(crate) fn slot_for_key(key: &str) -> Option<&'static ModelSlot> {
    SLOTS.iter().find(|s| s.key == key)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TierModels {
    fast: String,
    standard: String,
    strong: String,
}

impl TierModels {
    pub(crate) fn defaults_for(provider: LlmProvider) -> Self {
        let default_of = |tier: ModelTier| {
            SLOTS
                .iter()
                .find(|s| s.provider == provider && s.tier == tier)
                .map_or_else(String::new, |s| s.default.to_owned())
        };
        Self {
            fast: default_of(ModelTier::Fast),
            standard: default_of(ModelTier::Standard),
            strong: default_of(ModelTier::Strong),
        }
    }

    pub(crate) fn get(&self, tier: ModelTier) -> &str {
        match tier {
            ModelTier::Fast => &self.fast,
            ModelTier::Standard => &self.standard,
            ModelTier::Strong => &self.strong,
        }
    }

    pub(crate) fn set(&mut self, tier: ModelTier, model: String) {
        match tier {
            ModelTier::Fast => self.fast = model,
            ModelTier::Standard => self.standard = model,
            ModelTier::Strong => self.strong = model,
        }
    }
}

pub(crate) fn check_routes_to(provider: LlmProvider, model: &str) -> Result<(), String> {
    let Some(wanted) = provider.concrete() else {
        return Ok(());
    };
    let named = ModelRef::validate(model).map_err(|error| match error {
        ModelRefError::Blank => "cannot be blank".to_owned(),
        ModelRefError::EmptyName { prefix } => {
            format!("must name a model after `{prefix}`")
        }
    })?;
    if named.provider == wanted {
        return Ok(());
    }
    Err(match wanted {
        Provider::OpenRouter => format!(
            "must be an OpenRouter model, but `{}` models run through another provider",
            named.provider.prefix()
        ),
        Provider::Claude | Provider::Codex => format!(
            "must start with `{}` followed by a model name, because it is a {} model",
            wanted.prefix(),
            wanted.as_str()
        ),
    })
}
