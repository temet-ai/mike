use super::{SettingApplies, SettingKind, SettingMeta, SettingScope};

pub const MAX_ATTEMPTS_PER_DAY_KEY: &str = "session_judge.max_attempts_per_day";
pub const LIVE_BUNDLE_ID_KEY: &str = "session_judge.live_bundle_id";
pub const V3_LOOKBACK_HOURS_KEY: &str = "session_judge.v3_lookback_hours";
pub const REJUDGE_DAILY_CALL_CAP_KEY: &str = "session_judge.rejudge_daily_call_cap";
pub const SHADOW_BUNDLE_ID_KEY: &str = "session_end_shadow.bundle_id";
pub const SHADOW_SAMPLE_RATE_KEY: &str = "session_end_shadow.sample_rate";
pub const SHADOW_BATCH_SIZE_KEY: &str = "session_end_shadow.batch_size";
pub const SHADOW_LOOKBACK_HOURS_KEY: &str = "session_end_shadow.lookback_hours";
pub const SHADOW_INTERVAL_SECS_KEY: &str = "session_end_shadow.interval_secs";

fn int_meta(
    key: &'static str,
    label: &'static str,
    description: &'static str,
    default: i64,
    bounds: (i64, i64),
) -> SettingMeta {
    int_meta_applies(
        key,
        label,
        description,
        default,
        bounds,
        SettingApplies::NextCycle,
    )
}

fn int_meta_applies(
    key: &'static str,
    label: &'static str,
    description: &'static str,
    default: i64,
    bounds: (i64, i64),
    applies: SettingApplies,
) -> SettingMeta {
    SettingMeta {
        key,
        group: "Session judge",
        kind: SettingKind::Int,
        scope: SettingScope::Global,
        applies,
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

fn shadow_sample_rate_meta() -> SettingMeta {
    SettingMeta {
        key: SHADOW_SAMPLE_RATE_KEY,
        group: "Session judge",
        kind: SettingKind::Float,
        scope: SettingScope::Global,
        applies: SettingApplies::NextCycle,
        default: serde_json::json!(0.2),
        label: "Shadow sample rate",
        description: "The share of settled, closed sessions the session-end shadow path judges \
             with the candidate bundle. 0 judges nothing; 1 judges every eligible session within \
             the lookback window.",
        min_i64: None,
        max_i64: None,
        min_f64: Some(0.0),
        max_f64: Some(1.0),
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
        int_meta(
            SHADOW_BUNDLE_ID_KEY,
            "Shadow session-judge bundle",
            "The eval.bundles id of the candidate v3 configuration the shadow path judges \
             alongside the live one. 0 means no candidate is configured, so the shadow path \
             does nothing even while its toggle is on.",
            0,
            (0, i64::MAX),
        ),
        shadow_sample_rate_meta(),
        int_meta(
            SHADOW_BATCH_SIZE_KEY,
            "Shadow batch size",
            "The most sessions the shadow path judges in one tick.",
            10,
            (1, 50),
        ),
        int_meta(
            SHADOW_LOOKBACK_HOURS_KEY,
            "Shadow selection lookback (hours)",
            "The shadow path only considers sessions whose ending turn settled within this many \
             hours, so switching it on never backfills history.",
            72,
            (1, 720),
        ),
        int_meta_applies(
            SHADOW_INTERVAL_SECS_KEY,
            "Shadow tick interval (seconds)",
            "How often the shadow path polls for sessions to judge. The timer is built at \
             startup, so a change here takes effect on the next restart.",
            300,
            (60, 86_400),
            SettingApplies::OnRestart,
        ),
    ]
}

/// Default daily cap on session-judge attempts (store::persistence::judge).
pub const DEFAULT_MAX_ATTEMPTS_PER_DAY: i64 = 3;
