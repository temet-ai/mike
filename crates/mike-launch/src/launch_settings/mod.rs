pub mod meta_work_registry;

use serde_json::Value;

use crate::config::ConfigSections;
use crate::settings::{FeatureOverlays, FeatureSettings};

pub static LAUNCH_SETTINGS: FeatureSettings = FeatureSettings {
    name: "launch",
    meta: meta_work_registry::entries,
    sections: &[],
    apply_override: no_override,
    apply_overlay: no_overlay,
    boot_toggles: no_boot_toggles,
};

pub fn register() {
    crate::settings::registry::register(&LAUNCH_SETTINGS);
}

fn no_override(_sections: &mut ConfigSections, _key: &str, _value: &Value) -> bool {
    false
}

fn no_overlay(_overlays: &mut FeatureOverlays, _key: &str, _value: &Value) -> bool {
    false
}

fn no_boot_toggles(_sections: &ConfigSections) -> Vec<(&'static str, bool)> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::meta_work_registry::WAKE_MODE_KEY;

    #[test]
    fn launch_settings_register_adds_the_wake_mode_entry_once() {
        super::register();
        super::register();
        let matching = crate::settings::SETTINGS_META
            .iter()
            .filter(|meta| meta.key == WAKE_MODE_KEY)
            .count();
        assert_eq!(matching, 1);
    }
}
