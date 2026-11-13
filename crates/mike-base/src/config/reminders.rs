use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct RemindersConfig {
    pub enabled: bool,

    pub inject_lessons: bool,

    pub max_lessons_to_inject: usize,
}

impl Default for RemindersConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            inject_lessons: false,
            max_lessons_to_inject: 10,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_disabled() {
        let cfg = RemindersConfig::default();
        assert!(!cfg.enabled);
        assert!(!cfg.inject_lessons);
        assert_eq!(cfg.max_lessons_to_inject, 10);
    }

    #[test]
    fn deserialize_minimal_toml_keeps_default() -> anyhow::Result<()> {
        let parsed: RemindersConfig = toml::from_str("")?;
        assert_eq!(parsed, RemindersConfig::default());
        Ok(())
    }

    #[test]
    fn deserialize_enabled_true() -> anyhow::Result<()> {
        let parsed: RemindersConfig = toml::from_str("enabled = true\n")?;
        assert!(parsed.enabled);
        Ok(())
    }

    #[test]
    fn deserialize_inject_lessons_fields() -> anyhow::Result<()> {
        let parsed: RemindersConfig =
            toml::from_str("inject_lessons = true\nmax_lessons_to_inject = 5\n")?;
        assert!(parsed.inject_lessons);
        assert_eq!(parsed.max_lessons_to_inject, 5);
        Ok(())
    }

    #[test]
    fn deserialize_rejects_unknown_field() {
        let result: Result<RemindersConfig, _> =
            toml::from_str("enabled = true\nunknown_field = 1\n");
        assert!(result.is_err(), "unknown fields must be rejected");
    }
}
