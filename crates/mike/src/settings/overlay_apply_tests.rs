use super::*;

#[test]
fn apply_db_rows_to_overlay_folds_compression_dedup_enabled() {
    let rows = vec![(
        "compression_dedup.enabled".to_owned(),
        serde_json::Value::Bool(false),
    )];
    let overlay = apply_db_rows_to_overlay(rows);
    assert!(!overlay.compression_dedup_enabled);
    assert!(!overlay.reminders_inject_lessons);
}

#[test]
fn apply_db_rows_to_overlay_applies_known_keys() {
    let rows = vec![
        (
            "reminders.inject_lessons".to_owned(),
            serde_json::Value::Bool(false),
        ),
        (
            "reminders.max_lessons_to_inject".to_owned(),
            serde_json::Value::Number(10u64.into()),
        ),
    ];
    let overlay = apply_db_rows_to_overlay(rows);
    assert!(!overlay.reminders_inject_lessons);
    assert_eq!(overlay.reminders_max_lessons_to_inject, 10);
    assert_eq!(overlay.recall_injector_max_lessons_per_topic, 3);
}

#[test]
fn apply_db_rows_to_overlay_applies_session_judge_max_chars_per_turn() {
    let rows = vec![(
        "session_judge.max_chars_per_turn".to_owned(),
        serde_json::json!(5000),
    )];
    let overlay = apply_db_rows_to_overlay(rows);
    assert_eq!(overlay.session_judge_max_chars_per_turn, 5000);
}

#[test]
fn apply_db_rows_to_overlay_blank_correction_prompt_keeps_built_in() {
    let rows = vec![(
        "weasel_intercept.correction_prompt".to_owned(),
        serde_json::json!("   "),
    )];
    let overlay = apply_db_rows_to_overlay(rows);
    assert_eq!(
        overlay.weasel_intercept_correction_prompt,
        SettingsOverlay::default().weasel_intercept_correction_prompt,
        "a blank stored prompt must not overwrite the built-in one"
    );
}

#[test]
fn apply_db_rows_to_overlay_real_correction_prompt_is_used() {
    let rows = vec![(
        "weasel_intercept.correction_prompt".to_owned(),
        serde_json::json!("Don't say '{word}' again."),
    )];
    let overlay = apply_db_rows_to_overlay(rows);
    assert_eq!(
        overlay.weasel_intercept_correction_prompt,
        "Don't say '{word}' again."
    );
}

#[test]
fn apply_db_rows_to_overlay_blank_judge_system_prompt_yields_none() {
    let rows = vec![(
        "judge_worker.system_prompt".to_owned(),
        serde_json::json!(""),
    )];
    let overlay = apply_db_rows_to_overlay(rows);
    assert_eq!(overlay.judge_worker_system_prompt, None);
}

#[test]
fn apply_db_rows_to_overlay_ignores_unknown_keys() {
    let rows = vec![("no_such_key".to_owned(), serde_json::Value::Bool(true))];
    let overlay = apply_db_rows_to_overlay(rows);
    assert_eq!(overlay, SettingsOverlay::default());
}

#[test]
fn apply_db_rows_to_overlay_applies_file_read_compression_keys() {
    let rows = vec![
        (
            "file_read_compression.min_bytes".to_owned(),
            serde_json::json!(4000),
        ),
        (
            "file_read_compression.head_lines".to_owned(),
            serde_json::json!(10),
        ),
        (
            "file_read_compression.tail_lines".to_owned(),
            serde_json::json!(5),
        ),
        (
            "file_read_compression.enabled_producers".to_owned(),
            serde_json::json!(["Read", "Grep"]),
        ),
        (
            "file_read_compression.protected_paths".to_owned(),
            serde_json::json!(["**/*.env"]),
        ),
    ];
    let overlay = apply_db_rows_to_overlay(rows);
    assert_eq!(overlay.file_read_compression_min_bytes, 4000);
    assert_eq!(overlay.file_read_compression_head_lines, 10);
    assert_eq!(overlay.file_read_compression_tail_lines, 5);
    assert_eq!(
        overlay.file_read_compression_enabled_producers,
        vec!["Read".to_owned(), "Grep".to_owned()]
    );
    assert_eq!(
        overlay.file_read_compression_protected_paths,
        vec!["**/*.env".to_owned()]
    );
}

#[test]
fn live_steerer_model_follows_the_stored_setting_over_the_boot_model() {
    let rows = vec![(
        "steerer.model".to_owned(),
        serde_json::json!("codex/gpt-6-sol"),
    )];
    let overlay = apply_db_rows_to_overlay(rows);
    assert_eq!(
        overlay.live_steerer_model("openai/gpt-5.6-luna"),
        "codex/gpt-6-sol"
    );
}

#[test]
fn live_steerer_model_falls_back_to_the_boot_model_when_unset_or_blank() {
    let unset = apply_db_rows_to_overlay(Vec::new());
    assert_eq!(unset.live_steerer_model("boot/model"), "boot/model");
    let blank =
        apply_db_rows_to_overlay(vec![("steerer.model".to_owned(), serde_json::json!("  "))]);
    assert_eq!(blank.live_steerer_model("boot/model"), "boot/model");
}
