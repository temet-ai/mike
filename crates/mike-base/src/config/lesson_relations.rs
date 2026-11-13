use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LessonRelationsConfig {
    pub enabled: bool,

    pub proposer_model: String,

    pub adjudicator_model: String,

    pub candidate_distance_threshold: f64,

    pub request_timeout_secs: u64,
}

impl Default for LessonRelationsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            proposer_model: "openai/gpt-5.6-luna".to_string(),
            adjudicator_model: "google/gemini-3.5-flash-lite".to_string(),
            candidate_distance_threshold: 0.2,
            request_timeout_secs: 30,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lesson_relations_default_is_disabled_with_distinct_models() {
        let cfg = LessonRelationsConfig::default();
        assert!(!cfg.enabled);
        assert_ne!(
            cfg.proposer_model, cfg.adjudicator_model,
            "proposer and adjudicator must default to different models"
        );
    }

    #[test]
    fn lesson_relations_config_roundtrips_toml() -> anyhow::Result<()> {
        let cfg = LessonRelationsConfig::default();
        let toml = toml::to_string(&cfg)?;
        let parsed: LessonRelationsConfig = toml::from_str(&toml)?;
        assert_eq!(parsed.enabled, cfg.enabled);
        assert_eq!(parsed.proposer_model, cfg.proposer_model);
        assert_eq!(parsed.adjudicator_model, cfg.adjudicator_model);
        assert_eq!(
            parsed.candidate_distance_threshold,
            cfg.candidate_distance_threshold
        );
        assert_eq!(parsed.request_timeout_secs, cfg.request_timeout_secs);
        Ok(())
    }
}
