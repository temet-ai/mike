use super::{SettingApplies, SettingKind, SettingMeta, SettingScope};

pub const LIVE_BUNDLE_ID_KEY: &str = "turn_judge.live_bundle_id";
pub const IMMEDIATE_KEY: &str = "turn_judge.immediate";
pub const IMMEDIATE_DELAY_MS_KEY: &str = "turn_judge.immediate_delay_ms";
pub const LIVE_WINDOW_DAYS_KEY: &str = "turn_judge.live_window_days";
pub const REJUDGE_DAILY_CALL_CAP_KEY: &str = "turn_judge.rejudge_daily_call_cap";

pub const DEFAULT_IMMEDIATE_DELAY_MS: i64 = 1_500;
pub const DEFAULT_LIVE_WINDOW_DAYS: i64 = 7;
pub const DEFAULT_REJUDGE_DAILY_CALL_CAP: i64 = 300;

fn meta(
    key: &'static str,
    kind: SettingKind,
    default: serde_json::Value,
    text: (&'static str, &'static str),
) -> SettingMeta {
    SettingMeta {
        key,
        group: "Turn judge",
        kind,
        scope: SettingScope::Global,
        applies: SettingApplies::NextCycle,
        default,
        label: text.0,
        description: text.1,
        min_i64: None,
        max_i64: None,
        min_f64: None,
        max_f64: None,
        max_len: None,
        enum_variants: None,
    }
}

fn int(
    key: &'static str,
    default: i64,
    bounds: (i64, i64),
    text: (&'static str, &'static str),
) -> SettingMeta {
    SettingMeta {
        min_i64: Some(bounds.0),
        max_i64: Some(bounds.1),
        ..meta(
            key,
            SettingKind::Int,
            serde_json::Value::from(default),
            text,
        )
    }
}

pub fn entries() -> Vec<SettingMeta> {
    vec![
        int(
            LIVE_BUNDLE_ID_KEY,
            0,
            (0, i64::MAX),
            (
                "Live turn-judge bundle",
                "The eval.bundles id whose gated configuration writes every turn's label. 0 keeps \
                 the r3 judge. Setting a bundle id is the promotion: the judge then reads model, \
                 prompt and input builder from that bundle, never from the judge_worker settings.",
            ),
        ),
        meta(
            IMMEDIATE_KEY,
            SettingKind::Bool,
            serde_json::Value::Bool(false),
            (
                "Judge a turn as soon as it is answered",
                "When on, and a live bundle is set, a turn is judged a few seconds after the \
                 operator's next genuine message instead of on the next sweep. It adds no calls; \
                 it changes when turn text leaves the machine.",
            ),
        ),
        int(
            IMMEDIATE_DELAY_MS_KEY,
            DEFAULT_IMMEDIATE_DELAY_MS,
            (0, 30_000),
            (
                "Immediate judging delay (ms)",
                "How long the immediate judge waits after a turn closes before it builds the \
                 turn's facts and judges it.",
            ),
        ),
        int(
            LIVE_WINDOW_DAYS_KEY,
            DEFAULT_LIVE_WINDOW_DAYS,
            (1, 60),
            (
                "Live judge window (days)",
                "The sweep judges turns closed in this many days. Older turns belong to the \
                 history re-judge.",
            ),
        ),
        int(
            REJUDGE_DAILY_CALL_CAP_KEY,
            DEFAULT_REJUDGE_DAILY_CALL_CAP,
            (1, 5_000),
            (
                "History re-judge calls per day",
                "The most judge calls the history re-judge makes in one UTC day. \
                 mike eval rejudge --daily-cap overrides it for a run.",
            ),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        IMMEDIATE_DELAY_MS_KEY, IMMEDIATE_KEY, LIVE_BUNDLE_ID_KEY, LIVE_WINDOW_DAYS_KEY,
        REJUDGE_DAILY_CALL_CAP_KEY,
    };
    use crate::settings::SETTINGS_META;

    #[test]
    fn the_turn_judge_keys_are_registered_and_the_classifier_keys_are_gone() {
        let keys: Vec<&str> = SETTINGS_META.iter().map(|m| m.key).collect();
        for key in [
            LIVE_BUNDLE_ID_KEY,
            IMMEDIATE_KEY,
            IMMEDIATE_DELAY_MS_KEY,
            LIVE_WINDOW_DAYS_KEY,
            REJUDGE_DAILY_CALL_CAP_KEY,
        ] {
            assert!(keys.contains(&key), "{key} is not registered");
        }
    }

    #[test]
    fn the_promotion_setting_defaults_to_no_bundle_and_immediate_off() {
        let meta = |key: &str| {
            SETTINGS_META
                .iter()
                .find(|m| m.key == key)
                .map(|m| m.default.clone())
        };
        assert_eq!(meta(LIVE_BUNDLE_ID_KEY), Some(serde_json::json!(0)));
        assert_eq!(meta(IMMEDIATE_KEY), Some(serde_json::json!(false)));
    }
}
