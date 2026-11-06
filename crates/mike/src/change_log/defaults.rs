use serde_json::{Map, Value, json};

use crate::config::ProxyConfig;
use crate::feature_toggles::{TOGGLE_META, get_toggle_enabled};
use crate::settings::SETTINGS_META;
use crate::settings::service::is_secret_setting_key;

pub(crate) fn code_defaults(config: &ProxyConfig) -> Value {
    let mut settings = Map::new();
    for meta in SETTINGS_META.iter() {
        let value = if is_secret_setting_key(meta.key) {
            json!({"redacted": true})
        } else {
            meta.default.clone()
        };
        settings.insert(meta.key.to_owned(), value);
    }
    let overlay = crate::config::build_feature_toggle_overlay(config);
    let toggles_now = overlay.load();
    let mut toggles = Map::new();
    for meta in TOGGLE_META {
        if let Some(enabled) = get_toggle_enabled(&toggles_now, meta.key) {
            toggles.insert(meta.key.to_owned(), json!({"enabled": enabled}));
        }
    }
    json!({"settings": settings, "toggles": toggles})
}
