use serde_json::Value;

use super::sections::{LessonRelationsConfig, LessonWorkerConfig};
use crate::config::ConfigSections;
use crate::feature_toggles::FeatureToggles;

use super::sections::{HarvesterConfig, RecallInjectorConfig};

pub fn apply_override(sections: &mut ConfigSections, key: &str, value: &Value) -> bool {
    apply_lesson_worker_override(sections, key, value)
        || apply_lesson_relations_override(sections, key, value)
}

pub fn boot_toggles(sections: &ConfigSections, toggles: &mut FeatureToggles) {
    toggles.recall_injector = sections.get::<RecallInjectorConfig>().enabled;
    toggles.lesson_worker = sections.get::<LessonWorkerConfig>().enabled;
    toggles.harvester = sections.get::<HarvesterConfig>().enabled;
}

fn apply_lesson_worker_override(sections: &mut ConfigSections, key: &str, value: &Value) -> bool {
    let Some(field) = key.strip_prefix("lesson_worker.") else {
        return false;
    };
    let mut handled = true;
    sections.update::<LessonWorkerConfig>(|cfg| match field {
        "dedup_threshold" => {
            if let Some(v) = value.as_f64() {
                cfg.dedup_threshold = v as f32;
            }
        }
        "max_lessons_per_project" => {
            if let Some(v) = value.as_i64() {
                cfg.max_lessons_per_project = v;
            }
        }
        "session_consolidation_idle_secs" => {
            if let Some(v) = value.as_u64() {
                cfg.session_consolidation_idle_secs = v;
            }
        }
        "nightly_consolidation_hour" => {
            if let Some(v) = value.as_u64() {
                cfg.nightly_consolidation_hour = v as u8;
            }
        }
        "gc_interval_secs" => {
            if let Some(v) = value.as_u64() {
                cfg.gc_interval_secs = v;
            }
        }
        _ => handled = false,
    });
    handled
}

fn apply_lesson_relations_override(
    sections: &mut ConfigSections,
    key: &str,
    value: &Value,
) -> bool {
    let Some(field) = key.strip_prefix("lesson_relations.") else {
        return false;
    };
    let mut handled = true;
    sections.update::<LessonRelationsConfig>(|cfg| match field {
        "enabled" => {
            if let Some(v) = value.as_bool() {
                cfg.enabled = v;
            }
        }
        "proposer_model" => {
            if let Some(v) = value.as_str() {
                cfg.proposer_model = v.to_owned();
            }
        }
        "adjudicator_model" => {
            if let Some(v) = value.as_str() {
                cfg.adjudicator_model = v.to_owned();
            }
        }
        "candidate_distance_threshold" => {
            if let Some(v) = value.as_f64() {
                cfg.candidate_distance_threshold = v;
            }
        }
        _ => handled = false,
    });
    handled
}
