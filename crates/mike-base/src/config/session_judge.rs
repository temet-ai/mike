use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SessionJudgeConfig {
    pub enabled: bool,

    pub model: String,

    pub system_prompt: String,

    pub rubric_version: u32,

    pub poll_interval_secs: u64,

    pub min_session_age_secs: u64,

    pub request_timeout_secs: u64,

    pub max_turns_in_prompt: usize,

    pub batch_size: i64,

    pub circuit_breaker_threshold: u32,

    pub circuit_breaker_cooldown_base_secs: u64,

    pub circuit_breaker_cooldown_max_secs: u64,
}

impl Default for SessionJudgeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model: "google/gemini-3.5-flash-lite".to_string(),
            system_prompt: String::new(),
            rubric_version: 2,
            poll_interval_secs: 120,
            min_session_age_secs: 60,
            request_timeout_secs: 30,
            max_turns_in_prompt: 40,
            batch_size: 25,
            circuit_breaker_threshold: 5,
            circuit_breaker_cooldown_base_secs: 120,
            circuit_breaker_cooldown_max_secs: 1800,
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn session_judge_default_is_disabled() {
        let cfg = SessionJudgeConfig::default();
        assert!(!cfg.enabled);
    }

    #[test]
    fn session_judge_config_roundtrips_toml() -> anyhow::Result<()> {
        let cfg = SessionJudgeConfig::default();
        let toml = toml::to_string(&cfg)?;
        let parsed: SessionJudgeConfig = toml::from_str(&toml)?;
        assert_eq!(parsed.enabled, cfg.enabled);
        assert_eq!(parsed.model, cfg.model);
        assert_eq!(parsed.rubric_version, cfg.rubric_version);
        assert_eq!(parsed.poll_interval_secs, cfg.poll_interval_secs);
        assert_eq!(parsed.min_session_age_secs, cfg.min_session_age_secs);
        assert_eq!(parsed.request_timeout_secs, cfg.request_timeout_secs);
        assert_eq!(parsed.max_turns_in_prompt, cfg.max_turns_in_prompt);
        assert_eq!(parsed.batch_size, cfg.batch_size);
        assert_eq!(
            parsed.circuit_breaker_threshold,
            cfg.circuit_breaker_threshold
        );
        assert_eq!(
            parsed.circuit_breaker_cooldown_base_secs,
            cfg.circuit_breaker_cooldown_base_secs
        );
        assert_eq!(
            parsed.circuit_breaker_cooldown_max_secs,
            cfg.circuit_breaker_cooldown_max_secs
        );
        Ok(())
    }

    #[test]
    fn session_judge_circuit_breaker_defaults_are_sane() {
        let cfg = SessionJudgeConfig::default();
        assert!(cfg.circuit_breaker_threshold > 0);
        assert!(cfg.circuit_breaker_cooldown_base_secs > 0);
        assert!(cfg.circuit_breaker_cooldown_max_secs >= cfg.circuit_breaker_cooldown_base_secs);
    }
}
