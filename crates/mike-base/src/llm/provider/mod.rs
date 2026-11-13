use std::sync::{Arc, LazyLock};

use arc_swap::{ArcSwap, ArcSwapOption};

use super::model_ref::{ModelRef, ModelRefError, Provider};
use super::usage::LlmRole;
use crate::settings::SettingsOverlay;

pub mod models;
#[cfg(test)]
pub mod tests;

use models::TierModels;
pub use models::{SLOTS, slot_for_key};

pub const PROVIDER_KEY: &str = "llm.provider";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LlmProvider {
    #[default]
    PerRole,
    OpenRouter,
    Claude,
    Codex,
}

impl LlmProvider {
    pub const VARIANTS: &'static [&'static str] =
        &["per_role", "openrouter", "claude", "codex"];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::PerRole => "per_role",
            Self::OpenRouter => "openrouter",
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "per_role" => Some(Self::PerRole),
            "openrouter" => Some(Self::OpenRouter),
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::PerRole => "per role (each role uses its own model setting)",
            Self::OpenRouter => "OpenRouter",
            Self::Claude => "Claude (the machine's Claude Code login)",
            Self::Codex => "Codex (the machine's Codex login)",
        }
    }

    pub fn concrete(self) -> Option<Provider> {
        match self {
            Self::PerRole => None,
            Self::OpenRouter => Some(Provider::OpenRouter),
            Self::Claude => Some(Provider::Claude),
            Self::Codex => Some(Provider::Codex),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelTier {
    Fast,
    Standard,
    Strong,
}

impl ModelTier {
    #[cfg(any(test, feature = "test-seams"))]
    pub const ALL: [ModelTier; 3] = [Self::Fast, Self::Standard, Self::Strong];

    pub fn for_role(role: LlmRole) -> Option<Self> {
        match role {
            LlmRole::SessionJudge | LlmRole::LessonProposer => Some(Self::Fast),
            LlmRole::JudgeWorker => Some(Self::Standard),
            LlmRole::Steerer | LlmRole::LessonRelations => Some(Self::Strong),
            LlmRole::Eval | LlmRole::Unattributed => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderPolicy {
    provider: LlmProvider,
    openrouter: TierModels,
    claude: TierModels,
    codex: TierModels,
}

impl Default for ProviderPolicy {
    fn default() -> Self {
        Self {
            provider: LlmProvider::PerRole,
            openrouter: TierModels::defaults_for(LlmProvider::OpenRouter),
            claude: TierModels::defaults_for(LlmProvider::Claude),
            codex: TierModels::defaults_for(LlmProvider::Codex),
        }
    }
}

impl ProviderPolicy {
    pub fn provider(&self) -> LlmProvider {
        self.provider
    }

    fn tiers(&self, provider: LlmProvider) -> Option<&TierModels> {
        match provider {
            LlmProvider::OpenRouter => Some(&self.openrouter),
            LlmProvider::Claude => Some(&self.claude),
            LlmProvider::Codex => Some(&self.codex),
            LlmProvider::PerRole => None,
        }
    }

    fn tiers_mut(&mut self, provider: LlmProvider) -> Option<&mut TierModels> {
        match provider {
            LlmProvider::OpenRouter => Some(&mut self.openrouter),
            LlmProvider::Claude => Some(&mut self.claude),
            LlmProvider::Codex => Some(&mut self.codex),
            LlmProvider::PerRole => None,
        }
    }

    pub fn tier_models(&self, provider: Provider) -> Vec<&str> {
        let tiers = match provider {
            Provider::OpenRouter => &self.openrouter,
            Provider::Claude => &self.claude,
            Provider::Codex => &self.codex,
        };
        [ModelTier::Fast, ModelTier::Standard, ModelTier::Strong]
            .into_iter()
            .map(|tier| tiers.get(tier))
            .collect()
    }

    pub fn model_for(&self, tier: ModelTier) -> Option<&str> {
        self.tiers(self.provider).map(|models| models.get(tier))
    }

    pub fn resolve(&self, tier: Option<ModelTier>, own_model: &str) -> String {
        tier.and_then(|t| self.model_for(t))
            .unwrap_or(own_model)
            .to_owned()
    }

    pub fn set_provider(&mut self, value: &str) -> bool {
        match LlmProvider::parse(value) {
            Some(provider) => {
                self.provider = provider;
                true
            }
            None => false,
        }
    }

    pub fn set_model(&mut self, key: &str, value: &str) -> bool {
        let Some(slot) = models::slot_for_key(key) else {
            return false;
        };
        let value = value.trim();
        if value.is_empty() {
            return false;
        }
        match self.tiers_mut(slot.provider) {
            Some(models) => {
                models.set(slot.tier, value.to_owned());
                true
            }
            None => false,
        }
    }
}

static SOURCE: LazyLock<ArcSwapOption<ArcSwap<SettingsOverlay>>> =
    LazyLock::new(ArcSwapOption::empty);

pub fn install(overlay: Arc<ArcSwap<SettingsOverlay>>) {
    SOURCE.store(Some(overlay));
}

pub fn current() -> Arc<ProviderPolicy> {
    match SOURCE.load_full() {
        Some(overlay) => overlay.load().llm_provider.clone(),
        None => Arc::new(ProviderPolicy::default()),
    }
}

pub fn resolve(tier: Option<ModelTier>, own_model: &str) -> String {
    current().resolve(tier, own_model)
}

pub const MODEL_SETTING_KEYS: [&str; 6] = [
    "steerer.model",
    "judge_worker.model",
    "session_judge.model",
    "judge_lessons.model",
    "lesson_relations.proposer_model",
    "lesson_relations.adjudicator_model",
];

fn model_ref_problem(error: ModelRefError) -> String {
    match error {
        ModelRefError::Blank => "cannot be blank".to_owned(),
        ModelRefError::EmptyName { prefix } => {
            format!("names a provider but no model: `{prefix}` must be followed by a model name")
        }
    }
}

fn validate_model_setting(key: &str, value: &serde_json::Value) -> Result<(), String> {
    let model = value.as_str().map_or("", str::trim);
    if model.is_empty() {
        return Ok(());
    }
    ModelRef::validate(model)
        .map(|_| ())
        .map_err(|error| format!("setting '{key}' {}", model_ref_problem(error)))
}

pub fn validate_setting(key: &str, value: &serde_json::Value) -> Result<(), String> {
    let Some(slot) = models::slot_for_key(key) else {
        if MODEL_SETTING_KEYS.contains(&key) {
            return validate_model_setting(key, value);
        }
        return Ok(());
    };
    let model = value.as_str().map_or("", str::trim);
    if model.is_empty() {
        return Err(format!(
            "setting '{key}' cannot be blank; a provider default model must name a model"
        ));
    }
    models::check_routes_to(slot.provider, model)
        .map_err(|reason| format!("setting '{key}' {reason}"))
}
