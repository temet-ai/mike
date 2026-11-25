use serde_json::Value;

use super::sections::weasel_intercept::DEFAULT_CORRECTION_PROMPT;
use crate::settings::overlay_apply::{set_bool, set_prompt, set_string_list, set_usize};
use crate::settings::{FeatureOverlays, OverlaySection};

#[derive(Debug, Clone, PartialEq)]
pub struct RemindersOverlay {
    pub inject_lessons: bool,
    pub max_lessons_to_inject: usize,
}

impl Default for RemindersOverlay {
    fn default() -> Self {
        Self {
            inject_lessons: false,
            max_lessons_to_inject: 5,
        }
    }
}

impl OverlaySection for RemindersOverlay {}

#[derive(Debug, Clone, PartialEq)]
pub struct WeaselOverlay {
    pub forbidden_words: Vec<String>,
    pub correction_prompt: String,
}

impl Default for WeaselOverlay {
    fn default() -> Self {
        Self {
            forbidden_words: Vec::new(),
            correction_prompt: DEFAULT_CORRECTION_PROMPT.to_owned(),
        }
    }
}

impl OverlaySection for WeaselOverlay {}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ContextShapingOverlay {
    pub compression_dedup_enabled: bool,
}

impl OverlaySection for ContextShapingOverlay {}

pub fn apply_overlay(overlays: &mut FeatureOverlays, key: &str, value: &Value) -> bool {
    match key {
        "reminders.inject_lessons" => overlays.update::<RemindersOverlay>(|o| {
            set_bool(&mut o.inject_lessons, key, value);
        }),
        "reminders.max_lessons_to_inject" => overlays.update::<RemindersOverlay>(|o| {
            set_usize(&mut o.max_lessons_to_inject, key, value);
        }),
        "weasel_intercept.forbidden_words" => overlays.update::<WeaselOverlay>(|o| {
            set_string_list(&mut o.forbidden_words, key, value);
        }),
        "weasel_intercept.correction_prompt" => overlays.update::<WeaselOverlay>(|o| {
            set_prompt(&mut o.correction_prompt, key, value);
        }),
        "compression_dedup.enabled" => overlays.update::<ContextShapingOverlay>(|o| {
            set_bool(&mut o.compression_dedup_enabled, key, value);
        }),
        _ => return false,
    }
    true
}
