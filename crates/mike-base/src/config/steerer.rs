use secrecy::SecretString;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SteererConfig {
    pub enabled: bool,

    pub model: String,

    pub openrouter_base_url: String,

    #[serde(default, skip_serializing)]
    pub openrouter_api_key: Option<SecretString>,

    pub request_timeout_secs: u64,

    pub quiet_period_secs: u64,

    #[serde(default)]
    pub force_fire: bool,

    #[serde(default)]
    pub refocus: RefocusConfig,

    #[serde(default)]
    pub correction: CorrectionConfig,

    #[serde(default)]
    pub judge: JudgeConfig,

    #[serde(default = "default_payload_ttl_minutes")]
    pub payload_ttl_minutes: i64,

    #[serde(default = "default_quiescence_minutes")]
    pub quiescence_minutes: u64,
}

fn default_quiescence_minutes() -> u64 {
    5
}

fn default_payload_ttl_minutes() -> i64 {
    15
}

impl Default for SteererConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model: "openai/gpt-5.6-luna".to_string(),
            openrouter_base_url: "https://openrouter.ai/api/v1".to_string(),
            openrouter_api_key: None,
            request_timeout_secs: 8,
            quiet_period_secs: 20,
            force_fire: false,
            refocus: RefocusConfig::default(),
            correction: CorrectionConfig::default(),
            judge: JudgeConfig::default(),
            payload_ttl_minutes: default_payload_ttl_minutes(),
            quiescence_minutes: default_quiescence_minutes(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RefocusConfig {
    pub enabled: bool,

    pub confidence_threshold: f32,

    pub debounce_turns: u32,
}

impl Default for RefocusConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            confidence_threshold: 0.7,
            debounce_turns: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CorrectionConfig {
    pub enabled: bool,

    pub confidence_threshold: f32,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl Default for CorrectionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            confidence_threshold: 0.75,
            model: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct JudgeConfig {
    pub enabled: bool,

    pub confidence_threshold: f32,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    pub system_prompt: String,

    pub batch_size: i64,

    pub interval_secs: u64,

    pub backoff_base_secs: i64,

    pub backoff_max_secs: i64,

    pub circuit_breaker_threshold: u32,

    pub circuit_breaker_cooldown_base_secs: u64,

    pub circuit_breaker_cooldown_max_secs: u64,
}

impl Default for JudgeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            confidence_threshold: 0.5,
            system_prompt: String::new(),
            model: Some("google/gemini-3.8-flash".to_string()),
            batch_size: 50,
            interval_secs: 60,
            backoff_base_secs: 300,
            backoff_max_secs: 14_400,
            circuit_breaker_threshold: 5,
            circuit_breaker_cooldown_base_secs: 60,
            circuit_breaker_cooldown_max_secs: 1800,
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn steerer_default_is_disabled() {
        let cfg = SteererConfig::default();
        assert!(!cfg.enabled);
    }

    #[test]
    fn refocus_default_disabled() {
        let cfg = RefocusConfig::default();
        assert!(!cfg.enabled);
        assert!(cfg.confidence_threshold > 0.0);
        assert!(cfg.debounce_turns > 0);
    }

    #[test]
    fn correction_default_disabled() {
        let cfg = CorrectionConfig::default();
        assert!(!cfg.enabled);
        assert!(cfg.confidence_threshold > 0.0);
    }

    #[test]
    fn judge_default_model_is_gemini_3_8_flash() {
        assert_eq!(
            JudgeConfig::default().model.as_deref(),
            Some("google/gemini-3.8-flash")
        );
    }

    #[test]
    fn judge_default_disabled() {
        let cfg = JudgeConfig::default();
        assert!(!cfg.enabled);
        assert!(cfg.confidence_threshold > 0.0);
        assert!(cfg.batch_size > 0);
        assert!(cfg.interval_secs > 0);
        assert!(cfg.backoff_base_secs > 0);
        assert!(
            cfg.backoff_max_secs >= cfg.backoff_base_secs,
            "backoff ceiling must be at least the base"
        );
        assert!(cfg.circuit_breaker_threshold > 0);
        assert!(cfg.circuit_breaker_cooldown_base_secs > 0);
        assert!(cfg.circuit_breaker_cooldown_max_secs >= cfg.circuit_breaker_cooldown_base_secs);
    }

    #[test]
    fn steerer_config_roundtrips_toml() -> anyhow::Result<()> {
        let cfg = SteererConfig::default();
        let toml = toml::to_string(&cfg)?;
        let parsed: SteererConfig = toml::from_str(&toml)?;
        assert_eq!(parsed.enabled, cfg.enabled);
        assert_eq!(parsed.model, cfg.model);
        assert_eq!(parsed.request_timeout_secs, cfg.request_timeout_secs);
        assert_eq!(parsed.payload_ttl_minutes, cfg.payload_ttl_minutes);
        Ok(())
    }

    #[test]
    fn payload_ttl_minutes_defaults_to_15() {
        let cfg = SteererConfig::default();
        assert_eq!(cfg.payload_ttl_minutes, 15);
    }
}
