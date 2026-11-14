use crate::settings::{SettingApplies, SettingKind, SettingMeta, SettingScope};

use super::cross_session::{
    ALLOW_LOWERING_FOR_WAKE_KEY, CAL_MIN_LABELLED_KEY, CAL_WINDOW_DAYS_KEY, CEILING_KEY,
    COOLDOWN_DAYS_KEY, HYSTERESIS_KEY, INPUT_MODE_KEY, PROPOSER_MIN_CONFIRMED_KEY,
    PROPOSER_MIN_SESSIONS_KEY, PROPOSER_SOURCES_KEY, PROPOSER_WINDOW_DAYS_KEY, STEERER_BAR_KEY,
    THRESHOLD_MIN_LABELLED_KEY, THRESHOLD_MODE_KEY, WEIGHT_NEXT_MESSAGE_KEY, WEIGHT_OPERATOR_KEY,
    WEIGHT_SESSION_END_KEY, WEIGHT_SESSION_REPLY_KEY,
};

const GROUP: &str = "Steering: cross-session calibration";

fn base(
    key: &'static str,
    label: &'static str,
    description: &'static str,
    kind: SettingKind,
    default: serde_json::Value,
) -> SettingMeta {
    SettingMeta {
        key,
        group: GROUP,
        kind,
        scope: SettingScope::Global,
        applies: SettingApplies::NextCycle,
        default,
        label,
        description,
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
    label: &'static str,
    description: &'static str,
    default: i64,
    bounds: (i64, i64),
) -> SettingMeta {
    SettingMeta {
        min_i64: Some(bounds.0),
        max_i64: Some(bounds.1),
        ..base(
            key,
            label,
            description,
            SettingKind::Int,
            serde_json::Value::from(default),
        )
    }
}

fn float(
    key: &'static str,
    label: &'static str,
    description: &'static str,
    default: f64,
    bounds: (f64, f64),
) -> SettingMeta {
    SettingMeta {
        min_f64: Some(bounds.0),
        max_f64: Some(bounds.1),
        ..base(
            key,
            label,
            description,
            SettingKind::Float,
            serde_json::json!(default),
        )
    }
}

fn mode(
    key: &'static str,
    label: &'static str,
    description: &'static str,
    variants: &'static [&'static str],
) -> SettingMeta {
    SettingMeta {
        enum_variants: Some(variants),
        ..base(
            key,
            label,
            description,
            SettingKind::Enum,
            serde_json::json!("off"),
        )
    }
}

pub fn entries() -> Vec<SettingMeta> {
    let mut all = calibration_entries();
    all.extend(threshold_entries());
    all.extend(proposer_entries());
    all
}

fn calibration_entries() -> Vec<SettingMeta> {
    vec![
        mode(
            INPUT_MODE_KEY,
            "Calibration block in the guard's input",
            "off: the guard's call is unchanged. live: the guard's input carries a calibration block for its open questions and the fixed cal-v1 paragraph. Switch on only after mike guard replay --calibration shows precision at least as good with the block. Applies within a minute.",
            &["off", "live"],
        ),
        int(
            CAL_MIN_LABELLED_KEY,
            "Calibration: labelled candidates for a project line",
            "A project line needs this many labelled candidates; below it the block uses the all-projects line.",
            20,
            (1, 1000),
        ),
        int(
            CAL_WINDOW_DAYS_KEY,
            "Calibration window (days)",
            "The window the calibration rows cover, as stated in the block's header.",
            30,
            (1, 365),
        ),
    ]
}

fn threshold_entries() -> Vec<SettingMeta> {
    vec![
        mode(
            THRESHOLD_MODE_KEY,
            "Calibrated guard thresholds",
            "off: the guard uses each question's static threshold. apply: the daily pass sets each question's threshold from its labelled bins, within its floor and the ceiling, and writes every change to config.change_log.",
            &["off", "apply"],
        ),
        int(
            THRESHOLD_MIN_LABELLED_KEY,
            "Thresholds: labelled candidates needed",
            "A threshold is calibrated only from at least this many labelled candidates.",
            30,
            (1, 1000),
        ),
        float(
            HYSTERESIS_KEY,
            "Thresholds: hysteresis",
            "A candidate closer than this to the threshold in force is not applied.",
            0.05,
            (0.0, 0.5),
        ),
        int(
            COOLDOWN_DAYS_KEY,
            "Thresholds: cooldown (days)",
            "A threshold that changed less than this many days ago is held.",
            7,
            (0, 90),
        ),
        float(
            CEILING_KEY,
            "Thresholds: ceiling",
            "No calibrated threshold goes above this.",
            0.95,
            (0.5, 1.0),
        ),
        float(
            WEIGHT_OPERATOR_KEY,
            "Thresholds: weight of operator labels",
            "How much one label the operator gave counts toward a threshold's labelled total and precision. 1 counts it fully, 0 ignores the source.",
            1.0,
            (0.0, 1.0),
        ),
        float(
            WEIGHT_SESSION_REPLY_KEY,
            "Thresholds: weight of session replies",
            "How much a label taken from the session's own reply to a delivered note counts. 1 counts it fully, 0 ignores the source.",
            1.0,
            (0.0, 1.0),
        ),
        float(
            WEIGHT_SESSION_END_KEY,
            "Thresholds: weight of session-end labels",
            "How much a label inferred when the session ended counts. 1 counts it fully, 0 ignores the source.",
            0.8,
            (0.0, 1.0),
        ),
        float(
            WEIGHT_NEXT_MESSAGE_KEY,
            "Thresholds: weight of next-message labels",
            "How much a label inferred from the operator's next message counts. It is the weakest source because the next message may be about something else. 1 counts it fully, 0 ignores the source.",
            0.5,
            (0.0, 1.0),
        ),
        float(
            STEERER_BAR_KEY,
            "Thresholds: precision bar for the verifier and monitor",
            "The verifier and the monitor-over-sleep steerer raise their confidence threshold to the lowest value whose Wilson lower bound of precision clears this bar. When no confidence level clears it, the pass records below_bar and keeps the threshold in force.",
            0.75,
            (0.5, 0.99),
        ),
        base(
            ALLOW_LOWERING_FOR_WAKE_KEY,
            "Thresholds: allow lowering a wake threshold",
            "Lowering a wake question's threshold means more wakes; off keeps it at or above its static value.",
            SettingKind::Bool,
            serde_json::Value::Bool(false),
        ),
    ]
}

fn proposer_entries() -> Vec<SettingMeta> {
    vec![
        base(
            PROPOSER_SOURCES_KEY,
            "Directive proposer sources",
            "Which sources the daily proposer runs: repeated_directive, confirmed_handback. recurring_failure_tag and recurring_pattern are accepted and reported as unavailable.",
            SettingKind::StringList,
            serde_json::json!(["repeated_directive", "confirmed_handback"]),
        ),
        int(
            PROPOSER_WINDOW_DAYS_KEY,
            "Directive proposer window (days)",
            "Evidence must fall in this window.",
            30,
            (1, 365),
        ),
        int(
            PROPOSER_MIN_SESSIONS_KEY,
            "Directive proposer: sessions for a repeated directive",
            "A directive must be restated in at least this many sessions, one of them in a correcting message.",
            2,
            (1, 100),
        ),
        int(
            PROPOSER_MIN_CONFIRMED_KEY,
            "Directive proposer: confirmed hand-backs",
            "At least this many hand-backs the operator confirmed were authorised.",
            3,
            (1, 100),
        ),
    ]
}
