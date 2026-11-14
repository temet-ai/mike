use serde::{Deserialize, Serialize};

use crate::config::ConfigSection;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LessonWorkerConfig {
    pub enabled: bool,

    pub dedup_threshold: f32,

    pub max_lessons_per_project: i64,

    pub session_consolidation_idle_secs: u64,

    pub nightly_consolidation_hour: u8,

    pub gc_interval_secs: u64,
}

impl Default for LessonWorkerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            dedup_threshold: 0.85,
            max_lessons_per_project: 100,
            session_consolidation_idle_secs: 300,
            nightly_consolidation_hour: 3,
            gc_interval_secs: 3600,
        }
    }
}

impl ConfigSection for LessonWorkerConfig {
    const KEY: &'static str = "lesson_worker";
}

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

impl ConfigSection for RecallInjectorConfig {
    const KEY: &'static str = "recall_injector";
}

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

impl ConfigSection for HarvesterConfig {
    const KEY: &'static str = "harvester";
}

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

impl ConfigSection for LessonRelationsConfig {
    const KEY: &'static str = "lesson_relations";
}
