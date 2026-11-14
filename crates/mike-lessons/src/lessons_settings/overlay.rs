use serde_json::Value;

use crate::settings::overlay_apply::set_usize;
use crate::settings::{FeatureOverlays, OverlaySection};

pub const RECALL_MAX_LESSONS_PER_TOPIC_KEY: &str = "recall_injector.max_lessons_per_topic";

#[derive(Debug, Clone, PartialEq)]
pub struct RecallOverlay {
    pub max_lessons_per_topic: usize,
}

impl Default for RecallOverlay {
    fn default() -> Self {
        Self {
            max_lessons_per_topic: 3,
        }
    }
}

impl OverlaySection for RecallOverlay {}

pub fn apply_overlay(overlays: &mut FeatureOverlays, key: &str, value: &Value) -> bool {
    match key {
        RECALL_MAX_LESSONS_PER_TOPIC_KEY => {
            overlays.update::<RecallOverlay>(|overlay| {
                set_usize(&mut overlay.max_lessons_per_topic, key, value);
            });
        }
        _ => return false,
    }
    true
}
