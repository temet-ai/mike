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
        SHARE_KEY => match value.as_i64() {
            Some(pct) => overlay.llm_budget_decision_share_pct = pct,
            None => keep_default(key, value),
        },
        _ => return false,
    }
    true
}
