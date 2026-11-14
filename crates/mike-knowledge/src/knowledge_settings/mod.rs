pub mod meta_knowledge_embedding;
pub mod meta_search_ranking;
pub mod meta_semantic;

use serde_json::Value;

use crate::config::ConfigSections;
use crate::settings::{FeatureOverlays, FeatureSettings, SettingMeta};

pub static KNOWLEDGE_SETTINGS: FeatureSettings = FeatureSettings {
    name: "knowledge",
    meta: entries,
    sections: &[],
    apply_override: no_override,
    apply_overlay: no_overlay,
    boot_toggles: no_boot_toggles,
};

pub fn register() {
    crate::settings::registry::register(&KNOWLEDGE_SETTINGS);
}

fn entries() -> Vec<SettingMeta> {
    let mut entries = meta_semantic::entries();
    entries.extend(meta_knowledge_embedding::entries());
    entries.extend(meta_search_ranking::entries());
    entries
}

fn no_override(_: &mut ConfigSections, _: &str, _: &Value) -> bool {
    false
}

fn no_overlay(_: &mut FeatureOverlays, _: &str, _: &Value) -> bool {
    false
}

fn no_boot_toggles(_: &ConfigSections) -> Vec<(&'static str, bool)> {
    Vec::new()
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
