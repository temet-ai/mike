use serde_json::json;

use super::meta_rerank::entries;
use super::{SETTINGS_META, SettingMeta, validate_setting_value};
use crate::analysis::RerankConfig;

const KEYS: &[&str] = &[
    "rerank.enabled",
    "rerank.url",
    "rerank.model",
    "rerank.timeout_ms",
    "rerank.candidates",
    "rerank.tuning.enabled",
];

fn meta(key: &str) -> anyhow::Result<&'static SettingMeta> {
    SETTINGS_META
        .iter()
        .find(|m| m.key == key)
        .ok_or_else(|| anyhow::anyhow!("{key} must be registered in SETTINGS_META"))
}

#[test]
fn rerank_entries_register_every_planned_key_once() {
    let keys: Vec<&str> = entries().iter().map(|m| m.key).collect();
    assert_eq!(keys, KEYS);
    for key in KEYS {
        let registered = SETTINGS_META.iter().filter(|m| m.key == *key).count();
        assert_eq!(registered, 1, "{key} must be in SETTINGS_META exactly once");
    }
}

#[test]
fn rerank_defaults_mirror_the_compiled_config() -> anyhow::Result<()> {
    let compiled = RerankConfig::default();
    assert_eq!(meta("rerank.enabled")?.default, json!(compiled.enabled));
    assert_eq!(meta("rerank.url")?.default, json!(compiled.url));
    assert_eq!(meta("rerank.model")?.default, json!(compiled.model));
    assert_eq!(
        meta("rerank.timeout_ms")?.default,
        json!(compiled.timeout_ms)
    );
    assert_eq!(
        meta("rerank.candidates")?.default,
        json!(compiled.candidates)
    );
    assert_eq!(
        meta("rerank.tuning.enabled")?.default,
        json!(compiled.tuning_enabled)
    );
    Ok(())
}

#[test]
fn rerank_egress_and_tuning_default_off_on_a_fresh_install() -> anyhow::Result<()> {
    assert_eq!(meta("rerank.enabled")?.default, json!(false));
    assert_eq!(meta("rerank.tuning.enabled")?.default, json!(false));
    Ok(())
}

#[test]
fn rerank_integer_settings_reject_values_outside_their_bounds() {
    for (key, value) in [
        ("rerank.timeout_ms", 49),
        ("rerank.timeout_ms", 30_001),
        ("rerank.candidates", 0),
        ("rerank.candidates", 201),
    ] {
        let outcome = validate_setting_value(key, json!(value));
        assert!(outcome.is_err(), "{key}={value} must be rejected");
    }
    for (key, value) in [
        ("rerank.timeout_ms", 50),
        ("rerank.timeout_ms", 400),
        ("rerank.candidates", 1),
        ("rerank.candidates", 200),
    ] {
        let outcome = validate_setting_value(key, json!(value));
        assert!(
            outcome.is_ok(),
            "{key}={value} must be accepted: {outcome:?}"
        );
    }
}

#[test]
fn rerank_persisted_rows_fold_into_the_boot_config() {
    let mut config = crate::config::ProxyConfig::for_tests_in_memory();
    let rows = vec![
        ("rerank.enabled".to_owned(), json!(true)),
        ("rerank.url".to_owned(), json!("http://omit:11435")),
        ("rerank.model".to_owned(), json!("qwen3-reranker")),
        ("rerank.timeout_ms".to_owned(), json!(750)),
        ("rerank.candidates".to_owned(), json!(30)),
        ("rerank.tuning.enabled".to_owned(), json!(true)),
    ];

    crate::config::apply_settings_overrides(&mut config, &rows);

    assert_eq!(
        config.rerank,
        RerankConfig {
            enabled: true,
            url: "http://omit:11435".to_owned(),
            model: "qwen3-reranker".to_owned(),
            timeout_ms: 750,
            candidates: 30,
            tuning_enabled: true,
            ..RerankConfig::default()
        }
    );
}

#[test]
fn rerank_boot_config_without_rows_stays_disabled() {
    let mut config = crate::config::ProxyConfig::for_tests_in_memory();

    crate::config::apply_settings_overrides(&mut config, &[]);

    assert_eq!(config.rerank, RerankConfig::default());
}
