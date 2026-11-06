use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HarvesterConfig {
    pub enabled: bool,

    pub max_lesson_bytes: usize,
}

impl Default for HarvesterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_lesson_bytes: 4096,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_enabled() {
        let cfg = HarvesterConfig::default();
        assert!(
            cfg.enabled,
            "harvester must default to enabled (no-default-off rule)"
        );
        assert_eq!(cfg.max_lesson_bytes, 4096);
    }

    #[test]
    fn deserialize_minimal_toml_keeps_default_enabled() -> anyhow::Result<()> {
        let cfg: HarvesterConfig = toml::from_str("")?;
        assert!(cfg.enabled);
        Ok(())
    }
}
