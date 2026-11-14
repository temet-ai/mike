use serde::{Deserialize, Serialize};

use super::{ConfigSection, ConfigSections, SectionSpec};
use crate::config::ProxyConfig;
use crate::feature_toggles::FeatureToggles;
use crate::settings::registry::{self, FeatureSettings};
use crate::settings::{FeatureOverlays, SettingApplies, SettingKind, SettingMeta, SettingScope};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ProbeSection {
    enabled: bool,
    limit: u64,
}

impl Default for ProbeSection {
    fn default() -> Self {
        Self {
            enabled: true,
            limit: 7,
        }
    }
}

impl ConfigSection for ProbeSection {
    const KEY: &'static str = "probe";
}

fn probe_meta() -> Vec<SettingMeta> {
    vec![SettingMeta {
        key: "probe.limit",
        group: "Probe",
        kind: SettingKind::Int,
        scope: SettingScope::Global,
        applies: SettingApplies::OnRestart,
        default: serde_json::json!(7),
        label: "Probe limit",
        description: "A test setting.",
        min_i64: None,
        max_i64: None,
        min_f64: None,
        max_f64: None,
        max_len: None,
        enum_variants: None,
    }]
}

fn probe_override(sections: &mut ConfigSections, key: &str, value: &serde_json::Value) -> bool {
    if key != "probe.limit" {
        return false;
    }
    if let Some(limit) = value.as_u64() {
        sections.update::<ProbeSection>(|probe| probe.limit = limit);
    }
    true
}

fn no_overlay(_: &mut FeatureOverlays, _: &str, _: &serde_json::Value) -> bool {
    false
}

fn probe_toggles(sections: &ConfigSections, toggles: &mut FeatureToggles) {
    toggles.harvester = sections.get::<ProbeSection>().enabled;
}

static PROBE: FeatureSettings = FeatureSettings {
    name: "s7-probe",
    meta: probe_meta,
    sections: &[SectionSpec::of::<ProbeSection>()],
    apply_override: probe_override,
    apply_overlay: no_overlay,
    boot_toggles: probe_toggles,
};

#[test]
fn section_get_unregistered_returns_default() {
    let sections = ConfigSections::empty();
    assert_eq!(sections.get::<ProbeSection>(), ProbeSection::default());
}

#[test]
fn section_registered_appears_in_defaults_meta_and_serialisation() -> anyhow::Result<()> {
    registry::register(&PROBE);
    registry::register(&PROBE);
    let config = ProxyConfig::default();
    let json = serde_json::to_value(&config)?;
    assert_eq!(json["probe"]["limit"], serde_json::json!(7));
    let count = crate::settings::SETTINGS_META
        .iter()
        .filter(|meta| meta.key == "probe.limit")
        .count();
    assert_eq!(count, 1, "registering twice must not duplicate entries");
    let toml = crate::config::load::to_toml_string(&config)?;
    assert!(toml.contains("[probe]"), "{toml}");
    Ok(())
}

#[test]
fn section_registered_parses_file_values_and_rejects_wrong_types() -> anyhow::Result<()> {
    registry::register(&PROBE);
    let table: toml::Table = toml::from_str("[probe]\nlimit = 42\n")?;
    crate::config::load::file::validate(&table)?;
    let parsed: ProxyConfig = figment::Figment::from(figment::providers::Serialized::defaults(
        ProxyConfig::default(),
    ))
    .merge(figment::providers::Serialized::defaults(table))
    .extract()?;
    assert_eq!(parsed.sections.get::<ProbeSection>().limit, 42);
    assert!(parsed.sections.get::<ProbeSection>().enabled);

    let wrong: toml::Table = toml::from_str("[probe]\nlimit = \"many\"\n")?;
    assert!(crate::config::load::file::validate(&wrong).is_err());
    let unknown: toml::Table = toml::from_str("[probe]\nnope = 1\n")?;
    assert!(crate::config::load::file::validate(&unknown).is_err());
    Ok(())
}

#[test]
fn section_override_and_boot_toggle_go_through_the_feature() {
    registry::register(&PROBE);
    let mut config = ProxyConfig::default();
    crate::config::apply_settings_overrides(
        &mut config,
        &[("probe.limit".to_owned(), serde_json::json!(11))],
    );
    assert_eq!(config.sections.get::<ProbeSection>().limit, 11);
    config
        .sections
        .update::<ProbeSection>(|probe| probe.enabled = false);
    let toggles = crate::feature_toggles::build_feature_toggle_overlay(&config);
    assert!(!toggles.load().harvester);
}
