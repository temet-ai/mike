use super::*;

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
