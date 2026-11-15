use serde::{Deserialize, Serialize};

pub const DEFAULT_CORRECTION_PROMPT: &str = "Your reply was cut just before the word '{word}', \
     which is not accepted here. Continue from exactly where it was cut, without repeating \
     any of it and without that word.";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct WeaselInterceptConfig {
    pub enabled: bool,
    pub forbidden_words: Vec<String>,
    pub correction_prompt: String,
}

impl Default for WeaselInterceptConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            forbidden_words: Vec::new(),
            correction_prompt: String::from(DEFAULT_CORRECTION_PROMPT),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_disabled() {
        let cfg = WeaselInterceptConfig::default();
        assert!(!cfg.enabled);
        assert!(cfg.forbidden_words.is_empty());
    }

    #[test]
    fn deserialize_minimal_toml_keeps_default() -> anyhow::Result<()> {
        let cfg: WeaselInterceptConfig = toml::from_str("")?;
        assert!(!cfg.enabled);
        assert!(cfg.forbidden_words.is_empty());
        Ok(())
    }

    #[test]
    fn deserialize_enabled_true() -> anyhow::Result<()> {
        let cfg: WeaselInterceptConfig = toml::from_str(
            r#"
            enabled = true
            forbidden_words = ["comprehensive", "straightforward"]
            "#,
        )?;
        assert!(cfg.enabled);
        assert_eq!(cfg.forbidden_words, ["comprehensive", "straightforward"]);
        Ok(())
    }

    #[test]
    fn deserialize_rejects_unknown_field() {
        let result: Result<WeaselInterceptConfig, _> =
            toml::from_str("enabled = true\nunknown_key = 42");
        assert!(result.is_err());
    }
}
