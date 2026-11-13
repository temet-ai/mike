pub type LiveSettingsSource = std::sync::Arc<dyn Fn() -> LlmRuntimeSettings + Send + Sync>;

#[derive(Debug, Clone, PartialEq)]
pub struct LlmRuntimeSettings {
    pub reasoning_effort: String,
    pub max_output_tokens: u32,
    pub spend_alert_fraction: f64,
    pub record_usage: bool,
}

impl Default for LlmRuntimeSettings {
    fn default() -> Self {
        Self {
            reasoning_effort: "none".to_owned(),
            max_output_tokens: 800,
            spend_alert_fraction: 0.75,
            record_usage: true,
        }
    }
}
