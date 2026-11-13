use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RecallInjectorConfig {
    pub enabled: bool,

    pub max_lessons_per_topic: usize,
}

impl Default for RecallInjectorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_lessons_per_topic: 5,
        }
    }
}
