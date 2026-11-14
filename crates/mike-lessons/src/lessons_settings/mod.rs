pub mod meta;
pub mod meta_memory_loop;
pub mod overlay;
pub mod overrides;
pub mod sections;

pub use overlay::RecallOverlay;
pub use sections::{
    HarvesterConfig, LessonRelationsConfig, LessonWorkerConfig, RecallInjectorConfig,
};

use crate::config::SectionSpec;
use crate::settings::FeatureSettings;

pub static LESSONS_SETTINGS: FeatureSettings = FeatureSettings {
    name: "lessons",
    meta: meta::entries,
    sections: &[
        SectionSpec::of::<LessonWorkerConfig>(),
        SectionSpec::of::<RecallInjectorConfig>(),
        SectionSpec::of::<HarvesterConfig>(),
        SectionSpec::of::<LessonRelationsConfig>(),
    ],
    apply_override: overrides::apply_override,
    apply_overlay: overlay::apply_overlay,
    boot_toggles: overrides::boot_toggles,
};

pub fn register() {
    crate::settings::registry::register(&LESSONS_SETTINGS);
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
