use serde_json::json;

use super::super::SettingsOverlay;
use super::super::meta_turn_judge::{
    IMMEDIATE_DELAY_MS_KEY, IMMEDIATE_KEY, LIVE_BUNDLE_ID_KEY, LIVE_WINDOW_DAYS_KEY,
    REJUDGE_DAILY_CALL_CAP_KEY,
};
use super::apply_turn_judge_row;

fn applied(key: &str, value: serde_json::Value) -> SettingsOverlay {
    let mut overlay = SettingsOverlay::default();
    assert!(apply_turn_judge_row(&mut overlay, key, &value), "{key}");
    overlay
}

#[test]
fn defaults_keep_the_r3_path_and_immediate_off() {
    let overlay = SettingsOverlay::default();
    assert_eq!(overlay.turn_judge_live_bundle_id, 0);
    assert!(!overlay.turn_judge_immediate);
    assert_eq!(overlay.turn_judge_immediate_delay_ms, 1_500);
    assert_eq!(overlay.turn_judge_live_window_days, 7);
    assert_eq!(overlay.turn_judge_rejudge_daily_call_cap, 300);
}

#[test]
fn live_bundle_id_row_lands_on_the_overlay() {
    assert_eq!(
        applied(LIVE_BUNDLE_ID_KEY, json!(42)).turn_judge_live_bundle_id,
        42
    );
}

#[test]
fn immediate_row_lands_on_the_overlay() {
    assert!(applied(IMMEDIATE_KEY, json!(true)).turn_judge_immediate);
}

#[test]
fn immediate_delay_row_lands_on_the_overlay() {
    assert_eq!(
        applied(IMMEDIATE_DELAY_MS_KEY, json!(900)).turn_judge_immediate_delay_ms,
        900
    );
}

#[test]
fn live_window_days_row_lands_on_the_overlay() {
    assert_eq!(
        applied(LIVE_WINDOW_DAYS_KEY, json!(3)).turn_judge_live_window_days,
        3
    );
}

#[test]
fn rejudge_daily_cap_row_lands_on_the_overlay() {
    assert_eq!(
        applied(REJUDGE_DAILY_CALL_CAP_KEY, json!(120)).turn_judge_rejudge_daily_call_cap,
        120
    );
}

#[test]
fn a_wrongly_typed_value_keeps_the_default() {
    assert!(!applied(IMMEDIATE_KEY, json!("yes")).turn_judge_immediate);
    assert_eq!(
        applied(LIVE_BUNDLE_ID_KEY, json!("7")).turn_judge_live_bundle_id,
        0
    );
}

#[test]
fn unknown_turn_judge_key_falls_through() {
    let mut overlay = SettingsOverlay::default();
    assert!(!apply_turn_judge_row(
        &mut overlay,
        "turn_judge.unknown",
        &json!(1)
    ));
    assert!(!apply_turn_judge_row(
        &mut overlay,
        "judge_worker.model",
        &json!("x")
    ));
}
