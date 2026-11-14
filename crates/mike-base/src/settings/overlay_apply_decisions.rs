use super::SettingsOverlay;
use super::meta_decisions::{
    MODEL_KEY, MODES, TIMEOUT_MS_KEY, TURN_GUARD_HOLD_BELOW_KEY, TURN_GUARD_MODE_KEY,
};

const SHARE_KEY: &str = "llm.budget.decision.share_pct";

fn keep_default(key: &str, value: &serde_json::Value) {
    tracing::warn!(
        key,
        "settings: unexpected value {value} for {key}; keeping default"
    );
}

pub fn apply_decisions_row(
    overlay: &mut SettingsOverlay,
    key: &str,
    value: &serde_json::Value,
) -> bool {
    match key {
        MODEL_KEY => match value.as_str().map(str::trim).filter(|s| !s.is_empty()) {
            Some(model) => model.clone_into(&mut overlay.decisions_model),
            None => keep_default(key, value),
        },
        TIMEOUT_MS_KEY => match value.as_i64() {
            Some(ms) => overlay.decisions_timeout_ms = ms,
            None => keep_default(key, value),
        },
        TURN_GUARD_MODE_KEY => match value.as_str().filter(|m| MODES.contains(m)) {
            Some(mode) => mode.clone_into(&mut overlay.decisions_turn_guard_mode),
            None => keep_default(key, value),
        },
        TURN_GUARD_HOLD_BELOW_KEY => match value.as_f64() {
            Some(t) => overlay.decisions_turn_guard_hold_below = t,
            None => keep_default(key, value),
        },
        super::meta_decisions::RATE_KEY => match value.as_i64() {
            Some(n) => overlay.decisions_rate_per_minute = n,
            None => keep_default(key, value),
        },
        super::meta_decisions::HOURLY_KEY => match value.as_i64() {
            Some(n) => overlay.decisions_max_calls_per_hour = n,
            None => keep_default(key, value),
        },
        super::meta_decisions::DAILY_SPEND_KEY => match value.as_f64() {
            Some(usd) => overlay.decisions_daily_spend_ceiling_usd = usd,
            None => keep_default(key, value),
        },
        SHARE_KEY => match value.as_i64() {
            Some(pct) => overlay.llm_budget_decision_share_pct = pct,
            None => keep_default(key, value),
        },
        other => return apply_point_mode(overlay, other, value),
    }
    true
}

fn apply_point_mode(overlay: &mut SettingsOverlay, key: &str, value: &serde_json::Value) -> bool {
    let Some(point) = super::meta_decisions::point_setting(key) else {
        return false;
    };
    match value.as_str().filter(|m| MODES.contains(m)) {
        Some(mode) => {
            overlay
                .decisions_point_modes
                .insert(point.point.to_owned(), mode.to_owned());
        }
        None => keep_default(key, value),
    }
    true
}
