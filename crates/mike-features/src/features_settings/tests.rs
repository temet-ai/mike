use serde_json::json;

use super::{
    ContextShapingOverlay, FileReadCompressionOverlay, RemindersOverlay,
    ShellOutputCompressionOverlay, WeaselOverlay,
};
use crate::settings::{SettingsOverlay, apply_db_rows_to_overlay};

fn overlay_from(rows: Vec<(&str, serde_json::Value)>) -> SettingsOverlay {
    super::register();
    apply_db_rows_to_overlay(
        rows.into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

#[test]
fn features_overlay_reminders_keys_fold_into_reminders_slot() {
    let overlay = overlay_from(vec![
        ("reminders.inject_lessons", json!(true)),
        ("reminders.max_lessons_to_inject", json!(10)),
    ]);
    let reminders = overlay.features.get::<RemindersOverlay>();
    assert!(reminders.inject_lessons);
    assert_eq!(reminders.max_lessons_to_inject, 10);
}

#[test]
fn features_overlay_compression_dedup_folds_into_context_shaping() {
    let overlay = overlay_from(vec![("compression_dedup.enabled", json!(true))]);
    assert!(
        overlay
            .features
            .get::<ContextShapingOverlay>()
            .compression_dedup_enabled
    );
    assert!(!overlay.features.get::<RemindersOverlay>().inject_lessons);
}

#[test]
fn features_overlay_blank_correction_prompt_keeps_built_in() {
    let overlay = overlay_from(vec![("weasel_intercept.correction_prompt", json!("   "))]);
    assert_eq!(
        overlay.features.get::<WeaselOverlay>().correction_prompt,
        WeaselOverlay::default().correction_prompt,
    );
}

#[test]
fn features_overlay_real_correction_prompt_is_used() {
    let overlay = overlay_from(vec![(
        "weasel_intercept.correction_prompt",
        json!("Don't say '{word}' again."),
    )]);
    assert_eq!(
        overlay.features.get::<WeaselOverlay>().correction_prompt,
        "Don't say '{word}' again."
    );
}

#[test]
fn features_overlay_file_read_compression_keys_fold() {
    let overlay = overlay_from(vec![
        ("file_read_compression.min_bytes", json!(4000)),
        ("file_read_compression.head_lines", json!(10)),
        ("file_read_compression.tail_lines", json!(5)),
        (
            "file_read_compression.enabled_producers",
            json!(["Read", "Grep"]),
        ),
        ("file_read_compression.protected_paths", json!(["**/*.env"])),
        ("file_read_compression.mode", json!("forward")),
    ]);
    let file_read = overlay.features.get::<FileReadCompressionOverlay>();
    assert_eq!(file_read.min_bytes, 4000);
    assert_eq!(file_read.head_lines, 10);
    assert_eq!(file_read.tail_lines, 5);
    assert_eq!(file_read.enabled_producers, vec!["Read", "Grep"]);
    assert_eq!(file_read.protected_paths, vec!["**/*.env"]);
    assert!(file_read.forward());
}

#[test]
fn features_overlay_shell_output_keys_fold() {
    let overlay = overlay_from(vec![
        ("shell_output_compression.signal_line_cap", json!(12)),
        ("shell_output_compression.mode", json!("forward")),
    ]);
    let shell = overlay.features.get::<ShellOutputCompressionOverlay>();
    assert_eq!(shell.signal_line_cap, 12);
    assert!(shell.forward());
    assert_eq!(
        shell.head_lines,
        ShellOutputCompressionOverlay::default().head_lines
    );
}

#[test]
fn features_overlay_unknown_keys_leave_overlay_default() {
    let overlay = overlay_from(vec![("no_such_key", json!(true))]);
    assert_eq!(overlay, SettingsOverlay::default());
}

#[test]
fn features_settings_registered_meta_holds_moved_keys() {
    super::register();
    for key in [
        "reminders.inject_lessons",
        "weasel_intercept.correction_prompt",
        "file_read_compression.protected_paths",
        "shell_output_compression.mode",
        "compression_dedup.enabled",
        "context_focus_question.mode",
        super::meta_dossier::ENTRIES_ENABLED_KEY,
        super::meta_eval::HOLDOUT_PERCENT_KEY,
        super::meta_eval_alarms::EVAL_ROLLUP_ENABLED,
        super::meta_turn_guard::FLEET_SEATS_KEY,
        super::cross_session::STEERER_BAR_KEY,
    ] {
        assert!(
            crate::settings::SETTINGS_META.iter().any(|m| m.key == key),
            "{key} is not registered"
        );
    }
}

#[test]
fn features_settings_overlay_defaults_match_meta_defaults() {
    super::register();
    let meta_default = |key: &str| {
        crate::settings::SETTINGS_META
            .iter()
            .find(|m| m.key == key)
            .map(|m| m.default.clone())
    };
    assert_eq!(
        meta_default("reminders.max_lessons_to_inject"),
        Some(json!(RemindersOverlay::default().max_lessons_to_inject))
    );
    assert_eq!(
        meta_default("weasel_intercept.correction_prompt"),
        Some(json!(WeaselOverlay::default().correction_prompt))
    );
    assert_eq!(
        meta_default("file_read_compression.protected_paths"),
        Some(json!(FileReadCompressionOverlay::default().protected_paths))
    );
}

#[test]
fn features_settings_session_judge_override_updates_section() {
    let mut sections = crate::config::ConfigSections::default();
    assert!(super::overrides::apply_override(
        &mut sections,
        "session_judge.batch_size",
        &json!(7)
    ));
    assert!(!super::overrides::apply_override(
        &mut sections,
        "session_judge.not_a_field",
        &json!(7)
    ));
    assert!(!super::overrides::apply_override(
        &mut sections,
        "steerer.model",
        &json!("x")
    ));
    assert_eq!(sections.get::<super::SessionJudgeConfig>().batch_size, 7);
}
