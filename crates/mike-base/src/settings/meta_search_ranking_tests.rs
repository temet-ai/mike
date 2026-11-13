use serde_json::json;

use super::meta_search_ranking::entries;
use super::{SETTINGS_META, SettingKind, validate_setting_value};
use crate::search_ranking::{RANKING_KEYS, RankingConfig};

#[test]
fn ranking_entries_register_every_key_once_in_the_search_ranking_group() {
    let keys: Vec<&str> = entries().iter().map(|m| m.key).collect();
    assert_eq!(keys, RANKING_KEYS);
    for meta in entries() {
        let registered = SETTINGS_META.iter().filter(|m| m.key == meta.key).count();
        assert_eq!(registered, 1, "{} must be registered once", meta.key);
        assert_eq!(meta.group, "Search ranking");
    }
}

#[test]
fn ranking_defaults_mirror_the_compiled_config() {
    let compiled = RankingConfig::default();
    for meta in entries() {
        assert_eq!(
            Some(meta.default.clone()),
            compiled.value(meta.key),
            "{} default must mirror RankingConfig::default",
            meta.key
        );
    }
}

#[test]
fn validate_float_ranking_keys_enforce_their_range() {
    assert!(validate_setting_value("search.ranking.lexical_weight", json!(2.5)).is_ok());
    assert!(validate_setting_value("search.ranking.lexical_weight", json!(5.5)).is_err());
    assert!(validate_setting_value("search.ranking.semantic_floor", json!(1.2)).is_err());
    assert!(validate_setting_value("search.ranking.semantic_floor", json!(0.45)).is_ok());
    assert!(validate_setting_value("search.ranking.semantic_floor", json!(null)).is_ok());
    assert!(validate_setting_value("search.ranking.semantic_window", json!(0.25)).is_err());
    assert!(validate_setting_value("search.ranking.semantic_window", json!(0.12)).is_ok());
    assert!(validate_setting_value("search.ranking.semantic_window", json!(null)).is_ok());
    assert!(validate_setting_value("search.ranking.lexical_weight", json!(null)).is_err());
}

#[test]
fn validate_kind_weights_accepts_numbers_in_range_and_names_the_bad_entry() {
    assert!(validate_setting_value("search.ranking.kind_weights", json!({})).is_ok());
    assert!(
        validate_setting_value(
            "search.ranking.kind_weights",
            json!({"turn": 1.5, "note": 0})
        )
        .is_ok()
    );

    let too_high = validate_setting_value("search.ranking.kind_weights", json!({"turn": 9}))
        .err()
        .unwrap_or_default();
    assert!(
        too_high.contains("\"turn\"") && too_high.contains("<= 5"),
        "{too_high}"
    );

    let not_number = validate_setting_value("search.ranking.kind_weights", json!({"turn": "x"}))
        .err()
        .unwrap_or_default();
    assert!(not_number.contains("\"turn\""), "{not_number}");

    assert!(validate_setting_value("search.ranking.kind_weights", json!([1])).is_err());
}

#[test]
fn kind_weights_is_the_only_json_setting() {
    let json_keys: Vec<&str> = SETTINGS_META
        .iter()
        .filter(|m| m.kind == SettingKind::Json)
        .map(|m| m.key)
        .collect();
    assert_eq!(json_keys, ["search.ranking.kind_weights"]);
}

#[test]
fn validate_kind_weights_accepts_a_kind_no_code_knows_about() {
    assert!(
        validate_setting_value(
            "search.ranking.kind_weights",
            json!({"unmet_ask": 2.0, "thread_entry": 0.5, "anything_else": 1.0})
        )
        .is_ok()
    );
}
