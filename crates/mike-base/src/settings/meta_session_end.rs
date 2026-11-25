use super::{SettingApplies, SettingKind, SettingMeta, SettingScope};

pub const MAX_ATTEMPTS_PER_DAY_KEY: &str = "session_judge.max_attempts_per_day";
pub const LIVE_BUNDLE_ID_KEY: &str = "session_judge.live_bundle_id";
pub const V3_LOOKBACK_HOURS_KEY: &str = "session_judge.v3_lookback_hours";
pub const REJUDGE_DAILY_CALL_CAP_KEY: &str = "session_judge.rejudge_daily_call_cap";

fn int_meta(
    key: &'static str,
    label: &'static str,
    description: &'static str,
    default: i64,
    bounds: (i64, i64),
) -> SettingMeta {
    SettingMeta {
        key,
        group: "Session judge",
        kind: SettingKind::Int,
        scope: SettingScope::Global,
        applies: SettingApplies::NextCycle,
        default: serde_json::Value::from(default),
        label,
        description,
        min_i64: Some(bounds.0),
        max_i64: Some(bounds.1),
        min_f64: None,
        max_f64: None,
        max_len: None,
        enum_variants: None,
    }
}

pub fn entries() -> Vec<SettingMeta> {
    vec![
        int_meta(
            MAX_ATTEMPTS_PER_DAY_KEY,
            "Session judge attempts per session per day",
            "The most session-judge calls one session can cost in a rolling 24 hours, per \
             model and rubric. A session at the cap is skipped until its window rolls over, so a \
             selection defect can never turn into a call storm.",
            DEFAULT_MAX_ATTEMPTS_PER_DAY,
            (1, 50),
        ),
        int_meta(
            LIVE_BUNDLE_ID_KEY,
            "Live session-judge bundle",
            "The eval.bundles id whose gated v3 configuration judges why sessions end. 0 keeps \
             the legacy session judge. Setting a bundle id is the promotion; the v3 judge then \
             reads its model and rubric from the bundle.",
            0,
            (0, i64::MAX),
        ),
        int_meta(
            V3_LOOKBACK_HOURS_KEY,
            "v3 selection lookback (hours)",
            "The v3 judge picks up session endings that settled within this many hours, so \
             promotion never re-judges history by itself.",
            72,
            (1, 720),
        ),
        int_meta(
            REJUDGE_DAILY_CALL_CAP_KEY,
            "Session-end re-judge calls per day",
            "The most calls a session-end history re-judge makes in one UTC day.",
            200,
            (1, 2_000),
        ),
    ]
}

pub const DEFAULT_MAX_ATTEMPTS_PER_DAY: i64 = 3;
