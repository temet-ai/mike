use std::collections::HashMap;

use serde_json::Value;

use super::jobs::{Job, Tier, running};
use crate::tui::client::{ProviderDto, ProvidersPayload};

pub(super) const PROVIDER_ORDER: [&str; 3] = ["claude", "codex", "openrouter"];

#[derive(Debug, Clone)]
pub(super) struct Source {
    pub(super) providers: Vec<ProviderDto>,
    pub(super) values: HashMap<String, Value>,
    pub(super) running: Vec<Job>,
    pub(super) turn_guard: bool,
    pub(super) service_installed: bool,
    pub(super) key_variable: String,
}

impl Source {
    pub(super) fn new(
        payload: ProvidersPayload,
        values: HashMap<String, Value>,
        switches: &[(String, bool)],
        lesson_relations: bool,
        service_installed: bool,
    ) -> Self {
        let key_variable = payload
            .providers
            .iter()
            .find_map(|p| p.key_env_var.clone())
            .unwrap_or_else(|| "MIKE_STEERER_OPENROUTER_API_KEY".to_string());
        Self {
            providers: payload.providers,
            values,
            running: running(switches, lesson_relations),
            turn_guard: switches.iter().any(|(k, on)| k == "turn_guard" && *on),
            service_installed,
            key_variable,
        }
    }

    pub(super) fn provider(&self, id: &str) -> Option<&ProviderDto> {
        self.providers.iter().find(|p| p.id == id)
    }

    pub(super) fn available(&self) -> Vec<&ProviderDto> {
        PROVIDER_ORDER
            .iter()
            .filter_map(|id| self.provider(id))
            .filter(|p| p.available)
            .collect()
    }

    pub(super) fn unavailable(&self) -> Vec<&ProviderDto> {
        PROVIDER_ORDER
            .iter()
            .filter_map(|id| self.provider(id))
            .filter(|p| !p.available)
            .collect()
    }

    pub(super) fn text(&self, key: &str) -> String {
        match self.values.get(key) {
            Some(Value::String(text)) => text.trim().to_string(),
            Some(Value::Number(number)) => number.to_string(),
            _ => String::new(),
        }
    }

    pub(super) fn model_options(&self, provider: &str) -> Vec<String> {
        let Some(found) = self.provider(provider) else {
            return Vec::new();
        };
        let mut good: Vec<String> = Vec::new();
        let mut rest: Vec<String> = Vec::new();
        for model in &found.models {
            if model.known_good == Some(true) {
                good.push(model.model_ref.clone());
            } else {
                rest.push(model.model_ref.clone());
            }
        }
        good.extend(rest);
        good
    }

    pub(super) fn tier_default(&self, provider: &str, tier: Tier) -> String {
        let key = format!("llm.provider_models.{provider}.{}", tier.name());
        let stored = self.text(&key);
        if !stored.is_empty() {
            return stored;
        }
        self.model_options(provider)
            .into_iter()
            .next()
            .unwrap_or_default()
    }

    pub(super) fn first_available(&self) -> Option<String> {
        self.available().first().map(|p| p.id.clone())
    }
}

pub(super) fn provider_of(model_ref: &str) -> &'static str {
    if model_ref.starts_with("claude/") {
        "claude"
    } else if model_ref.starts_with("codex/") {
        "codex"
    } else {
        "openrouter"
    }
}

pub(super) fn normalize(provider: &str, text: &str) -> String {
    let text = text.trim();
    match provider {
        "claude" if !text.starts_with("claude/") => format!("claude/{text}"),
        "codex" if !text.starts_with("codex/") => format!("codex/{text}"),
        "openrouter" if text.starts_with("claude/") || text.starts_with("codex/") => {
            text.to_string()
        }
        "openrouter" if !text.starts_with("openrouter/") => format!("openrouter/{text}"),
        _ => text.to_string(),
    }
}
