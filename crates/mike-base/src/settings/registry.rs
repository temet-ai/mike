use std::sync::{LazyLock, Mutex, PoisonError, RwLock};

use super::SettingMeta;
use super::feature_overlays::FeatureOverlays;
use crate::config::sections::{ConfigSections, SectionSpec};
use crate::feature_toggles::FeatureToggles;

pub struct FeatureSettings {
    pub name: &'static str,
    pub meta: fn() -> Vec<SettingMeta>,
    pub sections: &'static [SectionSpec],
    pub apply_override: fn(&mut ConfigSections, &str, &serde_json::Value) -> bool,
    pub apply_overlay: fn(&mut FeatureOverlays, &str, &serde_json::Value) -> bool,
    pub boot_toggles: fn(&ConfigSections, &mut FeatureToggles),
}

static BASE_META: LazyLock<Vec<SettingMeta>> = LazyLock::new(super::meta::base_entries);
static FEATURES: RwLock<&'static [&'static FeatureSettings]> = RwLock::new(&[]);
static META: RwLock<Option<&'static [SettingMeta]>> = RwLock::new(None);
static WRITER: Mutex<()> = Mutex::new(());

pub fn register(feature: &'static FeatureSettings) {
    let _writer = WRITER.lock().unwrap_or_else(PoisonError::into_inner);
    let current = features();
    if current.iter().any(|known| known.name == feature.name) {
        return;
    }
    let mut next: Vec<&'static FeatureSettings> = current.to_vec();
    next.push(feature);
    let mut meta = BASE_META.clone();
    for known in &next {
        meta.extend((known.meta)());
    }
    let next: &'static [&'static FeatureSettings] = Vec::leak(next);
    let meta: &'static [SettingMeta] = Vec::leak(meta);
    *META.write().unwrap_or_else(PoisonError::into_inner) = Some(meta);
    *FEATURES.write().unwrap_or_else(PoisonError::into_inner) = next;
}

pub fn features() -> &'static [&'static FeatureSettings] {
    *FEATURES.read().unwrap_or_else(PoisonError::into_inner)
}

pub fn meta() -> &'static [SettingMeta] {
    let registered = *META.read().unwrap_or_else(PoisonError::into_inner);
    registered.unwrap_or_else(|| BASE_META.as_slice())
}

pub fn section_specs() -> impl Iterator<Item = &'static SectionSpec> {
    features()
        .iter()
        .flat_map(|feature| feature.sections.iter())
}

pub fn apply_config_override(
    sections: &mut ConfigSections,
    key: &str,
    value: &serde_json::Value,
) -> bool {
    features()
        .iter()
        .any(|feature| (feature.apply_override)(sections, key, value))
}

pub fn apply_overlay_row(
    overlays: &mut FeatureOverlays,
    key: &str,
    value: &serde_json::Value,
) -> bool {
    features()
        .iter()
        .any(|feature| (feature.apply_overlay)(overlays, key, value))
}

pub fn apply_boot_toggles(sections: &ConfigSections, toggles: &mut FeatureToggles) {
    for feature in features() {
        (feature.boot_toggles)(sections, toggles);
    }
}

pub struct SettingsMeta;

impl std::ops::Deref for SettingsMeta {
    type Target = [SettingMeta];

    fn deref(&self) -> &[SettingMeta] {
        meta()
    }
}
