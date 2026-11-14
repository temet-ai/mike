pub mod sections;

pub use sections::ChannelLinkSweepConfig;

use serde_json::Value;

use crate::config::{ConfigSections, SectionSpec};
use crate::feature_toggles::FeatureToggles;
use crate::settings::{FeatureOverlays, FeatureSettings, SettingMeta};

pub static CHANNEL_LINK_SETTINGS: FeatureSettings = FeatureSettings {
    name: "channel_link",
    meta: no_meta,
    sections: &[SectionSpec::of::<ChannelLinkSweepConfig>()],
    apply_override: no_override,
    apply_overlay: no_overlay,
    boot_toggles: no_boot_toggles,
};

pub fn register() {
    crate::settings::registry::register(&CHANNEL_LINK_SETTINGS);
}

fn no_meta() -> Vec<SettingMeta> {
    Vec::new()
}

fn no_override(_sections: &mut ConfigSections, _key: &str, _value: &Value) -> bool {
    false
}

fn no_overlay(_overlays: &mut FeatureOverlays, _key: &str, _value: &Value) -> bool {
    false
}

fn no_boot_toggles(_sections: &ConfigSections, _toggles: &mut FeatureToggles) {}

#[cfg(test)]
mod tests {
    use super::ChannelLinkSweepConfig;
    use crate::config::ProxyConfig;

    #[test]
    fn channel_link_settings_toml_table_reaches_the_section() -> anyhow::Result<()> {
        super::register();
        let cfg: ProxyConfig =
            toml::from_str("[channel_link_sweep]\ninterval_secs = 5\nbatch_size = 7\n")?;
        let sweep = cfg.sections.get::<ChannelLinkSweepConfig>();
        assert_eq!((sweep.interval_secs, sweep.batch_size), (5, 7));
        Ok(())
    }

    #[test]
    fn channel_link_settings_unknown_field_fails_the_parse() {
        super::register();
        let parsed = toml::from_str::<ProxyConfig>("[channel_link_sweep]\nintervall = 5\n");
        assert!(parsed.is_err());
    }
}
