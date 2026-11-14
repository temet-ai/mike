pub mod cross_session;
pub mod meta;
pub mod meta_cross_session;
pub mod meta_dossier;
pub mod meta_eval;
pub mod meta_eval_alarms;
pub mod meta_session_scoped;
pub mod meta_shell_output;
pub mod meta_turn_guard;
pub mod overlay;
pub mod overrides;
pub mod sections;

pub use overlay::{
    ContextShapingOverlay, FileReadCompressionOverlay, RemindersOverlay,
    ShellOutputCompressionOverlay, WeaselOverlay,
};
pub use sections::{CompressConfig, RemindersConfig, SessionJudgeConfig, WeaselInterceptConfig};

use crate::config::SectionSpec;
use crate::settings::FeatureSettings;

pub static FEATURES_SETTINGS: FeatureSettings = FeatureSettings {
    name: "features",
    meta: meta::entries,
    sections: &[
        SectionSpec::of::<CompressConfig>(),
        SectionSpec::of::<RemindersConfig>(),
        SectionSpec::of::<WeaselInterceptConfig>(),
        SectionSpec::of::<SessionJudgeConfig>(),
    ],
    apply_override: overrides::apply_override,
    apply_overlay: overlay::apply_overlay,
    boot_toggles: overrides::boot_toggles,
};

pub fn register() {
    crate::settings::registry::register(&FEATURES_SETTINGS);
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
#[path = "validate_tests.rs"]
mod validate_tests;
