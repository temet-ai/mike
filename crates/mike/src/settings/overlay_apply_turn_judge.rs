use super::SettingsOverlay;
use super::meta_turn_judge::{
    IMMEDIATE_DELAY_MS_KEY, IMMEDIATE_KEY, LIVE_BUNDLE_ID_KEY, LIVE_WINDOW_DAYS_KEY,
    REJUDGE_DAILY_CALL_CAP_KEY,
};

fn int(field: &mut i64, key: &str, value: &serde_json::Value) {
    match value.as_i64() {
        Some(n) => *field = n,
        None => tracing::warn!(
            key,
            "settings: expected int for {key}, got {value}; keeping default"
        ),
    }
}

pub(super) fn apply_turn_judge_row(
    overlay: &mut SettingsOverlay,
    key: &str,
    value: &serde_json::Value,
) -> bool {
    match key {
        LIVE_BUNDLE_ID_KEY => int(&mut overlay.turn_judge_live_bundle_id, key, value),
        IMMEDIATE_KEY => match value.as_bool() {
            Some(b) => overlay.turn_judge_immediate = b,
            None => tracing::warn!(
                key,
                "settings: expected bool for {key}, got {value}; keeping default"
            ),
        },
        IMMEDIATE_DELAY_MS_KEY => int(&mut overlay.turn_judge_immediate_delay_ms, key, value),
        LIVE_WINDOW_DAYS_KEY => int(&mut overlay.turn_judge_live_window_days, key, value),
        REJUDGE_DAILY_CALL_CAP_KEY => {
            int(&mut overlay.turn_judge_rejudge_daily_call_cap, key, value);
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
#[path = "overlay_apply_turn_judge_tests.rs"]
mod tests;
