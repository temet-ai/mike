use serde_json::json;

use super::settings::{RoleInputs, expected_roles_from, resolve_eval_settings};

#[test]
fn resolve_eval_settings_no_rows_uses_meta_defaults() {
    let s = resolve_eval_settings(&[]);
    assert!(s.rollup_enabled);
    assert_eq!(s.rollup_hour_utc, 3);
    assert_eq!(s.mix_min_turns, 50);
    assert!((s.mix_min_tvd - 0.20).abs() < 1e-9);
    assert_eq!(s.backlog_min_turns, 10);
    assert_eq!(s.gate_grace_hours, 24);
}

#[test]
fn resolve_eval_settings_out_of_bounds_falls_back_to_default() {
    let s = resolve_eval_settings(&[("eval.rollup.hour_utc".to_owned(), json!(31))]);
    assert_eq!(s.rollup_hour_utc, 3);
}

#[test]
fn resolve_eval_settings_wrong_type_falls_back_to_default() {
    let s = resolve_eval_settings(&[("eval.rollup.enabled".to_owned(), json!("yes"))]);
    assert!(s.rollup_enabled);
}

#[test]
fn resolve_eval_settings_reads_a_valid_stored_value() {
    let s = resolve_eval_settings(&[(
        "eval.alarm.change_without_gate.grace_hours".to_owned(),
        json!(0),
    )]);
    assert_eq!(s.gate_grace_hours, 0);
}

#[test]
fn resolve_eval_settings_ignores_jc07_eval_keys() {
    let s = resolve_eval_settings(&[("eval.labels.holdout_percent".to_owned(), json!(90))]);
    assert_eq!(s, resolve_eval_settings(&[]));
}

#[test]
fn resolve_eval_settings_label_band_defaults_off_with_operator_census_bounds() {
    let s = resolve_eval_settings(&[]);
    assert!(!s.band_enabled);
    assert!((s.band_min - 0.32).abs() < 1e-9);
    assert!((s.band_max - 0.49).abs() < 1e-9);
}

#[test]
fn resolve_eval_settings_reads_a_stored_label_band_enabled_value() {
    let s = resolve_eval_settings(&[("eval.alarm.label_band.enabled".to_owned(), json!(true))]);
    assert!(s.band_enabled);
}

#[test]
fn resolve_eval_settings_promotion_alarms_default_on_with_jc10_defaults() {
    let s = resolve_eval_settings(&[]);
    assert!(s.shadow_disagreement_enabled);
    assert_eq!(s.shadow_disagreement_min_n, 50);
    assert!((s.shadow_disagreement_min_kappa - 0.80).abs() < 1e-9);
    assert!(s.metric_drop_enabled);
    assert_eq!(s.metric_drop_min_n, 20);
}

#[test]
fn resolve_eval_settings_reads_a_stored_metric_drop_disabled_value() {
    let s = resolve_eval_settings(&[("eval.alarm.metric_drop.enabled".to_owned(), json!(false))]);
    assert!(!s.metric_drop_enabled);
}

fn all_on() -> RoleInputs {
    RoleInputs {
        judge_toggle: true,
        judge_config_enabled: true,
        session_judge_toggle: true,
        steerer_toggle: true,
    }
}

#[test]
fn expected_roles_judge_toggle_off_excludes_turn_judge() {
    let roles = expected_roles_from(RoleInputs {
        judge_toggle: false,
        ..all_on()
    });
    assert!(!roles.contains(&"turn_judge"));
}

#[test]
fn expected_roles_judge_config_disabled_excludes_turn_judge() {
    let roles = expected_roles_from(RoleInputs {
        judge_config_enabled: false,
        ..all_on()
    });
    assert!(!roles.contains(&"turn_judge"));
}

#[test]
fn expected_roles_never_include_the_retired_classifier_or_fast_path() {
    let roles = expected_roles_from(all_on());
    assert!(!roles.contains(&"correction_classifier"));
    assert!(!roles.contains(&"turn_fast_path"));
}
