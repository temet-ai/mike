use super::SettingsOverlay;
use super::meta_session_end::{
    LIVE_BUNDLE_ID_KEY, MAX_ATTEMPTS_PER_DAY_KEY, REJUDGE_DAILY_CALL_CAP_KEY,
    SHADOW_BATCH_SIZE_KEY, SHADOW_BUNDLE_ID_KEY, SHADOW_INTERVAL_SECS_KEY,
    SHADOW_LOOKBACK_HOURS_KEY, SHADOW_SAMPLE_RATE_KEY, V3_LOOKBACK_HOURS_KEY,
};
use super::overlay_apply::set_optional_i64;

pub fn apply_session_end_row(
    overlay: &mut SettingsOverlay,
    key: &str,
    value: &serde_json::Value,
) -> bool {
    match key {
        MAX_ATTEMPTS_PER_DAY_KEY => {
            set_optional_i64(&mut overlay.session_judge_max_attempts_per_day, key, value);
        }
        LIVE_BUNDLE_ID_KEY => {
            overlay.session_judge_live_bundle_id = value
                .as_i64()
                .unwrap_or(overlay.session_judge_live_bundle_id);
        }
        V3_LOOKBACK_HOURS_KEY => {
            overlay.session_judge_v3_lookback_hours = value
                .as_i64()
                .unwrap_or(overlay.session_judge_v3_lookback_hours);
        }
        REJUDGE_DAILY_CALL_CAP_KEY => {
            overlay.session_judge_rejudge_daily_call_cap = value
                .as_i64()
                .unwrap_or(overlay.session_judge_rejudge_daily_call_cap);
        }
        SHADOW_BUNDLE_ID_KEY => {
            overlay.session_end_shadow_bundle_id = value
                .as_i64()
                .unwrap_or(overlay.session_end_shadow_bundle_id);
        }
        SHADOW_SAMPLE_RATE_KEY => {
            overlay.session_end_shadow_sample_rate = value
                .as_f64()
                .unwrap_or(overlay.session_end_shadow_sample_rate);
        }
        SHADOW_BATCH_SIZE_KEY => {
            overlay.session_end_shadow_batch_size = value
                .as_i64()
                .unwrap_or(overlay.session_end_shadow_batch_size);
        }
        SHADOW_LOOKBACK_HOURS_KEY => {
            overlay.session_end_shadow_lookback_hours = value
                .as_i64()
                .unwrap_or(overlay.session_end_shadow_lookback_hours);
        }
        SHADOW_INTERVAL_SECS_KEY => {
            overlay.session_end_shadow_interval_secs = value
                .as_i64()
                .unwrap_or(overlay.session_end_shadow_interval_secs);
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_attempts_per_day_row_lands_on_the_overlay() {
        let mut overlay = SettingsOverlay::default();
        assert!(apply_session_end_row(
            &mut overlay,
            MAX_ATTEMPTS_PER_DAY_KEY,
            &serde_json::json!(5)
        ));
        assert_eq!(overlay.session_judge_max_attempts_per_day, Some(5));
        assert!(apply_session_end_row(
            &mut overlay,
            LIVE_BUNDLE_ID_KEY,
            &serde_json::json!(9)
        ));
        assert_eq!(overlay.session_judge_live_bundle_id, 9);
        assert!(apply_session_end_row(
            &mut overlay,
            V3_LOOKBACK_HOURS_KEY,
            &serde_json::json!(24)
        ));
        assert_eq!(overlay.session_judge_v3_lookback_hours, 24);
        assert!(!apply_session_end_row(
            &mut overlay,
            "session_judge.model",
            &serde_json::json!("x")
        ));
    }

    #[test]
    fn shadow_rows_land_on_the_overlay() {
        let mut overlay = SettingsOverlay::default();
        assert!(apply_session_end_row(
            &mut overlay,
            SHADOW_BUNDLE_ID_KEY,
            &serde_json::json!(42)
        ));
        assert_eq!(overlay.session_end_shadow_bundle_id, 42);
        assert!(apply_session_end_row(
            &mut overlay,
            SHADOW_SAMPLE_RATE_KEY,
            &serde_json::json!(0.5)
        ));
        assert!((overlay.session_end_shadow_sample_rate - 0.5).abs() < 1e-9);
        assert!(apply_session_end_row(
            &mut overlay,
            SHADOW_BATCH_SIZE_KEY,
            &serde_json::json!(3)
        ));
        assert_eq!(overlay.session_end_shadow_batch_size, 3);
        assert!(apply_session_end_row(
            &mut overlay,
            SHADOW_LOOKBACK_HOURS_KEY,
            &serde_json::json!(48)
        ));
        assert_eq!(overlay.session_end_shadow_lookback_hours, 48);
        assert!(apply_session_end_row(
            &mut overlay,
            SHADOW_INTERVAL_SECS_KEY,
            &serde_json::json!(600)
        ));
        assert_eq!(overlay.session_end_shadow_interval_secs, 600);
    }
}
