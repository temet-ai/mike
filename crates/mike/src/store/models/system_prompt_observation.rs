use super::SystemPromptSource;

#[derive(Debug, Clone, Default)]
pub struct SystemPromptObservation {
    pub prompt_sha256: Option<String>,
    pub blocks: Option<serde_json::Value>,
    pub composition_id: Option<i64>,
    pub composition_applied: bool,
    pub unknown_blocks: Option<serde_json::Value>,
    pub filter_modified: bool,
    pub firing_rule_ids: Vec<i64>,
    pub source: SystemPromptSource,
}
