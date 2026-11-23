use super::settings::Settings;
use crate::knowledge_settings::meta_knowledge_consolidation as keys;
use serde_json::json;
use std::time::Duration;

fn row(key: &str, value: serde_json::Value) -> (String, serde_json::Value) {
    (key.to_owned(), value)
}

#[test]
fn no_rows_give_the_documented_defaults() {
    let s = Settings::default();
    assert_eq!(s.interval, Duration::from_secs(60));
    assert_eq!(s.items_per_tick, 8);
    assert_eq!(s.subjects_per_tick, 4);
    assert_eq!(s.settle, chrono::Duration::seconds(120));
    assert_eq!(s.max_judgements_per_day, 400);
    assert_eq!(s.model, "");
    assert!((s.grouping_similarity - 0.70).abs() < 1e-6);
    assert!(!s.edit_files);
}

#[test]
fn stored_values_apply_and_out_of_range_values_are_clamped() {
    let s = Settings::from_rows(&[
        row(keys::INTERVAL_SECS, json!(5)),
        row(keys::ITEMS_PER_TICK, json!(1000)),
        row(keys::MODEL, json!("  claude/opus ")),
        row(keys::GROUPING_SIMILARITY, json!(0.1)),
        row(keys::EDIT_FILES, json!(true)),
        row(keys::SETTLE_SECS, json!("soon")),
    ]);
    assert_eq!(s.interval, Duration::from_secs(10));
    assert_eq!(s.items_per_tick, 64);
    assert_eq!(s.model, "claude/opus");
    assert!((s.grouping_similarity - 0.5).abs() < 1e-6);
    assert!(s.edit_files);
    assert_eq!(s.settle, chrono::Duration::seconds(120));
}

#[test]
fn every_key_is_registered_in_settings_meta_once() {
    crate::knowledge_settings::register();
    for key in [
        keys::INTERVAL_SECS,
        keys::ITEMS_PER_TICK,
        keys::SUBJECTS_PER_TICK,
        keys::SETTLE_SECS,
        keys::MAX_JUDGEMENTS_PER_DAY,
        keys::MODEL,
        keys::GROUPING_SIMILARITY,
        keys::EDIT_FILES,
    ] {
        assert_eq!(
            crate::settings::SETTINGS_META.iter().filter(|m| m.key == key).count(),
            1,
            "{key}"
        );
    }
}
