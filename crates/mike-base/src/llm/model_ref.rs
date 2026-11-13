use std::fmt;

use super::{claude, codex};

pub const OPENROUTER_PREFIX: &str = "openrouter/";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provider {
    OpenRouter,
    Claude,
    Codex,
}

impl Provider {
    pub const ALL: [Provider; 3] = [Self::OpenRouter, Self::Claude, Self::Codex];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenRouter => "openrouter",
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::OpenRouter => "OpenRouter",
            Self::Claude => "Claude",
            Self::Codex => "Codex",
        }
    }

    pub fn prefix(self) -> &'static str {
        match self {
            Self::OpenRouter => OPENROUTER_PREFIX,
            Self::Claude => claude::MODEL_PREFIX,
            Self::Codex => codex::MODEL_PREFIX,
        }
    }

    pub fn needs_openrouter_key(self) -> bool {
        self == Self::OpenRouter
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModelRefError {
    #[error("the model name is blank")]
    Blank,
    #[error("`{prefix}` must be followed by a model name")]
    EmptyName { prefix: &'static str },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRef {
    pub provider: Provider,
    pub model: String,
}

fn explicit_openrouter_name(rest: &str) -> bool {
    rest.split_once('/')
        .is_some_and(|(vendor, name)| !vendor.is_empty() && !name.is_empty())
}

impl ModelRef {
    pub fn new(provider: Provider, model: impl Into<String>) -> Self {
        Self {
            provider,
            model: model.into(),
        }
    }

    pub fn parse(stored: &str) -> Self {
        for provider in [Provider::Claude, Provider::Codex] {
            if let Some(name) = stored
                .strip_prefix(provider.prefix())
                .filter(|name| !name.is_empty())
            {
                return Self::new(provider, name);
            }
        }
        match stored.strip_prefix(OPENROUTER_PREFIX) {
            Some(name) if explicit_openrouter_name(name) => Self::new(Provider::OpenRouter, name),
            _ => Self::new(Provider::OpenRouter, stored),
        }
    }

    pub fn validate(stored: &str) -> Result<Self, ModelRefError> {
        let stored = stored.trim();
        if stored.is_empty() {
            return Err(ModelRefError::Blank);
        }
        for provider in Provider::ALL {
            if stored == provider.prefix() {
                return Err(ModelRefError::EmptyName {
                    prefix: provider.prefix(),
                });
            }
        }
        Ok(Self::parse(stored))
    }

    pub fn format(&self) -> String {
        match self.provider {
            Provider::Claude | Provider::Codex => {
                format!("{}{}", self.provider.prefix(), self.model)
            }
            Provider::OpenRouter if explicit_openrouter_name(&self.model) => {
                format!("{OPENROUTER_PREFIX}{}", self.model)
            }
            Provider::OpenRouter => self.model.clone(),
        }
    }
}

impl fmt::Display for ModelRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(stored: &str) -> ModelRef {
        ModelRef::parse(stored)
    }

    #[test]
    fn parse_claude_prefix_selects_claude_and_strips_it() {
        assert_eq!(
            parsed("claude/sonnet"),
            ModelRef::new(Provider::Claude, "sonnet")
        );
    }

    #[test]
    fn parse_codex_prefix_selects_codex_and_strips_it() {
        assert_eq!(
            parsed("codex/gpt-6-sol"),
            ModelRef::new(Provider::Codex, "gpt-6-sol")
        );
    }

    #[test]
    fn parse_explicit_openrouter_prefix_keeps_the_vendor_and_never_selects_a_cli() {
        let anthropic = parsed("openrouter/anthropic/claude-sonnet-5-5");
        assert_eq!(
            anthropic,
            ModelRef::new(Provider::OpenRouter, "anthropic/claude-sonnet-5-5")
        );
        let openai = parsed("openrouter/openai/gpt-6-sol");
        assert_eq!(
            openai,
            ModelRef::new(Provider::OpenRouter, "openai/gpt-6-sol")
        );
    }

    #[test]
    fn parse_unprefixed_name_is_openrouter_for_back_compat() {
        assert_eq!(
            parsed("google/gemini-3.8-flash"),
            ModelRef::new(Provider::OpenRouter, "google/gemini-3.8-flash")
        );
        assert_eq!(
            parsed("anthropic/claude-haiku"),
            ModelRef::new(Provider::OpenRouter, "anthropic/claude-haiku")
        );
    }

    #[test]
    fn parse_openrouter_own_model_ids_stay_whole() {
        assert_eq!(
            parsed("openrouter/auto"),
            ModelRef::new(Provider::OpenRouter, "openrouter/auto")
        );
        assert_eq!(
            parsed("openrouter/openrouter/auto"),
            ModelRef::new(Provider::OpenRouter, "openrouter/auto")
        );
    }

    #[test]
    fn parse_bare_cli_prefix_falls_back_to_openrouter_like_before() {
        assert_eq!(
            parsed("claude/"),
            ModelRef::new(Provider::OpenRouter, "claude/")
        );
        assert_eq!(
            parsed("codex/"),
            ModelRef::new(Provider::OpenRouter, "codex/")
        );
    }

    #[test]
    fn format_round_trips_every_provider() {
        let refs = [
            ModelRef::new(Provider::Claude, "haiku"),
            ModelRef::new(Provider::Claude, "claude-sonnet-5-5"),
            ModelRef::new(Provider::Codex, "gpt-6.1-sol"),
            ModelRef::new(Provider::OpenRouter, "anthropic/claude-sonnet-5-5"),
            ModelRef::new(Provider::OpenRouter, "openai/gpt-6-sol"),
            ModelRef::new(Provider::OpenRouter, "openrouter/auto"),
            ModelRef::new(Provider::OpenRouter, "claude/looks-like-a-cli"),
            ModelRef::new(Provider::OpenRouter, "plain"),
        ];
        for model_ref in refs {
            assert_eq!(
                ModelRef::parse(&model_ref.format()),
                model_ref,
                "{}",
                model_ref.format()
            );
        }
    }

    #[test]
    fn format_writes_openrouter_vendor_models_with_the_explicit_prefix() {
        assert_eq!(
            ModelRef::new(Provider::OpenRouter, "anthropic/claude-sonnet-5-5").format(),
            "openrouter/anthropic/claude-sonnet-5-5"
        );
        assert_eq!(
            ModelRef::new(Provider::Codex, "gpt-6-sol").format(),
            "codex/gpt-6-sol"
        );
        assert_eq!(
            ModelRef::new(Provider::Claude, "opus").format(),
            "claude/opus"
        );
    }

    #[test]
    fn format_of_stored_prefixed_values_is_unchanged() {
        for stored in [
            "claude/haiku",
            "codex/gpt-6-luna",
            "openrouter/anthropic/claude-sonnet-5-5",
        ] {
            assert_eq!(ModelRef::parse(stored).format(), stored);
        }
    }

    #[test]
    fn validate_rejects_blank_and_bare_prefixes() {
        assert_eq!(ModelRef::validate("  "), Err(ModelRefError::Blank));
        for prefix in ["claude/", "codex/", "openrouter/"] {
            assert_eq!(
                ModelRef::validate(prefix),
                Err(ModelRefError::EmptyName { prefix }),
                "{prefix}"
            );
        }
    }

    #[test]
    fn validate_accepts_every_provider_form() {
        for stored in [
            "claude/sonnet",
            "codex/gpt-6-sol",
            "openrouter/openai/gpt-6-sol",
            "google/gemini-3.8-flash",
            "openrouter/auto",
        ] {
            assert!(ModelRef::validate(stored).is_ok(), "{stored}");
        }
    }

    #[test]
    fn provider_names_and_prefixes_are_distinct() {
        let names: Vec<&str> = Provider::ALL.iter().map(|p| p.as_str()).collect();
        assert_eq!(names, vec!["openrouter", "claude", "codex"]);
        let prefixes: Vec<&str> = Provider::ALL.iter().map(|p| p.prefix()).collect();
        assert_eq!(prefixes, vec!["openrouter/", "claude/", "codex/"]);
    }

    #[test]
    fn only_openrouter_needs_the_openrouter_key() {
        assert!(Provider::OpenRouter.needs_openrouter_key());
        assert!(!Provider::Claude.needs_openrouter_key());
        assert!(!Provider::Codex.needs_openrouter_key());
    }
}
