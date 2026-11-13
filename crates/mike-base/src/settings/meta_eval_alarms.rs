use super::{SettingApplies, SettingKind, SettingMeta, SettingScope};

pub const EVAL_ROLLUP_ENABLED: &str = "eval.rollup.enabled";
pub const EVAL_ROLLUP_HOUR_UTC: &str = "eval.rollup.hour_utc";
pub const EVAL_LABEL_MIX_MIN_TURNS: &str = "eval.alarm.label_mix.min_turns";
pub const EVAL_LABEL_MIX_MIN_TVD: &str = "eval.alarm.label_mix.min_tvd";
pub const EVAL_LABEL_MIX_MIN_CHI2: &str = "eval.alarm.label_mix.min_chi2";
pub const EVAL_LABEL_BAND_ENABLED: &str = "eval.alarm.label_band.enabled";
pub const EVAL_LABEL_BAND_CORRECTED_MIN: &str = "eval.alarm.label_band.corrected_min";
pub const EVAL_LABEL_BAND_CORRECTED_MAX: &str = "eval.alarm.label_band.corrected_max";
pub const EVAL_BACKLOG_MIN_TURNS: &str = "eval.alarm.unjudged_backlog.min_turns";
pub const EVAL_STALLED_MIN_WAITING: &str = "eval.alarm.judge_stalled.min_waiting";
pub const EVAL_STALLED_HOURS: &str = "eval.alarm.judge_stalled.hours";
pub const EVAL_REFUSALS_MIN_SHARE: &str = "eval.alarm.budget_refusals.min_share";
pub const EVAL_REFUSALS_MIN_REFUSED: &str = "eval.alarm.budget_refusals.min_refused";
pub const EVAL_GATE_GRACE_HOURS: &str = "eval.alarm.change_without_gate.grace_hours";
pub const EVAL_CALL_LOOP_MIN_CALLS: &str = "eval.alarm.judge_call_loop.min_calls";
pub const EVAL_CALL_LOOP_MAX_PER_ROW: &str = "eval.alarm.judge_call_loop.max_calls_per_row";
pub const EVAL_SHADOW_DISAGREEMENT_ENABLED: &str = "eval.alarm.shadow_disagreement.enabled";
pub const EVAL_SHADOW_DISAGREEMENT_MIN_N: &str = "eval.alarm.shadow_disagreement.min_n";
pub const EVAL_SHADOW_DISAGREEMENT_MIN_KAPPA: &str =
    "eval.alarm.shadow_disagreement.min_kappa";
pub const EVAL_METRIC_DROP_ENABLED: &str = "eval.alarm.metric_drop.enabled";
pub const EVAL_METRIC_DROP_MIN_N: &str = "eval.alarm.metric_drop.min_n";

pub const EVAL_GROUP: &str = "Evaluation";

fn base(key: &'static str, kind: SettingKind, default: serde_json::Value) -> SettingMeta {
    SettingMeta {
        key,
        group: EVAL_GROUP,
        kind,
        scope: SettingScope::Global,
        applies: SettingApplies::NextCycle,
        default,
        label: "",
        description: "",
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
        label: text.0,
        description: text.1,
        min_i64: Some(bounds.0),
        max_i64: Some(bounds.1),
        ..base(key, SettingKind::Int, serde_json::Value::from(default))
    }
}

fn float(
    key: &'static str,
    default: f64,
    bounds: (f64, f64),
    text: (&'static str, &'static str),
) -> SettingMeta {
    SettingMeta {
        label: text.0,
        description: text.1,
        min_f64: Some(bounds.0),
        max_f64: Some(bounds.1),
        ..base(key, SettingKind::Float, serde_json::Value::from(default))
    }
}

fn rollup_entries() -> Vec<SettingMeta> {
    vec![
        SettingMeta {
            label: "Daily metrics rollup and alarms",
            description: "Local SQL only: no traffic mutation, no egress. Default on so drift is \
                caught without a prior opt-in. Off stops the rollup and every alarm at the next tick.",
            ..base(
                EVAL_ROLLUP_ENABLED,
                SettingKind::Bool,
                serde_json::Value::Bool(true),
            )
        },
        int(
            EVAL_ROLLUP_HOUR_UTC,
            3,
            (0, 23),
            (
                "Rollup hour (UTC)",
                "The rollup runs on the first tick at or after this UTC hour. 03:00 UTC is after \
                 midnight in every zone the operator works in, so the day's turns are closed.",
            ),
        ),
    ]
}

fn label_mix_entries() -> Vec<SettingMeta> {
    vec![
        int(
            EVAL_LABEL_MIX_MIN_TURNS,
            50,
            (10, 10_000),
            (
                "Label-mix shift: minimum turns",
                "Minimum judged real turns in the 7-day window (the baseline needs 3x). Below 50 a \
                 week's mix is noise: the thinnest census week had 92 turns. Local SQL only.",
            ),
        ),
        float(
            EVAL_LABEL_MIX_MIN_TVD,
            0.20,
            (0.01, 1.0),
            (
                "Label-mix shift: minimum TVD",
                "Total variation distance floor. Backtest: the settled stretch 2026-08-19 to 09-02 \
                 sat at 0.05-0.14; days firing on the two judge changes were at 0.21-0.39.",
            ),
        ),
        float(
            EVAL_LABEL_MIX_MIN_CHI2,
            40.0,
            (1.0, 10_000.0),
            (
                "Label-mix shift: minimum chi-squared",
                "Chi-squared floor, df 3 (p < 1e-7), so small-n swings do not fire. Backtest: \
                 firing days had chi-squared 45-238.",
            ),
        ),
        SettingMeta {
            label: "Label band: switch on",
            description: "Switches the band detector (subject band:turn_judge/corrected). Off \
                until a promoted judge backs the reaction axis: against stored labels the band \
                measures recall, not change, so on today it would be open from the first day \
                until promotion. The band row is still computed and shown either way. Local SQL \
                only, so either default is allowed.",
            ..base(
                EVAL_LABEL_BAND_ENABLED,
                SettingKind::Bool,
                serde_json::Value::Bool(false),
            )
        },
        float(
            EVAL_LABEL_BAND_CORRECTED_MIN,
            0.32,
            (0.0, 1.0),
            (
                "Label band: minimum corrected share",
                "The lowest weekly correction rate the operator showed on replied real turns \
                 (census, 2026-07-27 to 09-21).",
            ),
        ),
        float(
            EVAL_LABEL_BAND_CORRECTED_MAX,
            0.49,
            (0.0, 1.0),
            (
                "Label band: maximum corrected share",
                "The highest weekly correction rate the operator showed on replied real turns \
                 (census, 2026-07-27 to 09-21).",
            ),
        ),
    ]
}

fn operational_alarm_entries() -> Vec<SettingMeta> {
    vec![
        int(
            EVAL_BACKLOG_MIN_TURNS,
            10,
            (1, 100_000),
            (
                "Unjudged backlog: minimum stale turns",
                "Stale (over 6 h) unjudged turns needed to open an episode; /api/health keeps its \
                 own >0 light. Local SQL only.",
            ),
        ),
        int(
            EVAL_STALLED_MIN_WAITING,
            5,
            (1, 10_000),
            (
                "Judge stalled: minimum waiting turns",
                "Turns waiting more than 30 minutes before a silent judge counts as stalled, not \
                 merely idle.",
            ),
        ),
        int(
            EVAL_STALLED_HOURS,
            6,
            (1, 72),
            (
                "Judge stalled: hours without a verdict",
                "Hours with no verdict written. Matches the 6-hour health threshold.",
            ),
        ),
        float(
            EVAL_REFUSALS_MIN_SHARE,
            0.5,
            (0.05, 1.0),
            (
                "Budget refusals: minimum refused share",
                "Share of a role's calls refused over 24 h. Backtest: the judge sat at 64-91% when \
                 starved; the session judge at 20-33% in normal running.",
            ),
        ),
        int(
            EVAL_REFUSALS_MIN_REFUSED,
            50,
            (1, 100_000),
            (
                "Budget refusals: minimum refused calls",
                "Minimum refused calls, so a role making a handful of calls cannot fire.",
            ),
        ),
        int(
            EVAL_GATE_GRACE_HOURS,
            24,
            (0, 720),
            (
                "Change without gate: grace hours",
                "Time allowed to run a gate after a bundle change before the alarm opens. 0 makes \
                 it immediate.",
            ),
        ),
        int(
            EVAL_CALL_LOOP_MIN_CALLS,
            50,
            (1, 100_000),
            (
                "Judge call loop: minimum calls",
                "Session-judge calls in 24 hours below which the loop alarm never fires.",
            ),
        ),
        float(
            EVAL_CALL_LOOP_MAX_PER_ROW,
            3.0,
            (1.0, 100.0),
            (
                "Judge call loop: calls per verdict row",
                "The alarm fires when successful session-judge calls exceed this many per live verdict row in 24 hours. The 2026-09 storm ran at 24 to 2,347.",
            ),
        ),
    ]
}

fn promotion_alarm_entries() -> Vec<SettingMeta> {
    vec![
        SettingMeta {
            label: "Shadow disagreement: switch on",
            description: "Switches the shadow-disagreement detector (subject turn_judge:<live \
                bundle id>). Local SQL only, comparing stored live and shadow verdicts; no \
                traffic mutation, no egress. Default on, as JC08's other alarms are.",
            ..base(
                EVAL_SHADOW_DISAGREEMENT_ENABLED,
                SettingKind::Bool,
                serde_json::Value::Bool(true),
            )
        },
        int(
            EVAL_SHADOW_DISAGREEMENT_MIN_N,
            50,
            (1, 100_000),
            (
                "Shadow disagreement: minimum paired turns",
                "Minimum turns judged by both the live and the current shadow bundle in the \
                 trailing 14 days, while their components (ignoring role) still match, before \
                 kappa is trusted.",
            ),
        ),
        float(
            EVAL_SHADOW_DISAGREEMENT_MIN_KAPPA,
            0.80,
            (0.0, 1.0),
            (
                "Shadow disagreement: minimum Cohen's kappa",
                "Below this kappa on the reaction axis, the shadow bundle no longer agrees with \
                 the live bundle closely enough to trust the shadow arm's 14-day comparison \
                 (JC10's AC11 bar).",
            ),
        ),
        SettingMeta {
            label: "Metric drop: switch on",
            description: "Switches the metric-drop detector (subjects turn_judge:<live bundle \
                id>:precision and :recall). Local SQL only, reading eval.rates; no traffic \
                mutation, no egress. Default on, as JC08's other alarms are.",
            ..base(
                EVAL_METRIC_DROP_ENABLED,
                SettingKind::Bool,
                serde_json::Value::Bool(true),
            )
        },
        int(
            EVAL_METRIC_DROP_MIN_N,
            20,
            (1, 100_000),
            (
                "Metric drop: minimum labelled turns",
                "Minimum gold-or-relayed labelled turns in the live bundle's trailing 30-day \
                 accuracy rollup before precision or recall is trusted.",
            ),
        ),
    ]
}

pub fn entries() -> Vec<SettingMeta> {
    let mut entries = rollup_entries();
    entries.extend(label_mix_entries());
    entries.extend(operational_alarm_entries());
    entries.extend(promotion_alarm_entries());
    entries
}

#[cfg(test)]
pub mod tests {
    use super::{EVAL_GROUP, entries};

    #[test]
    fn meta_eval_alarms_every_default_within_bounds() {
        for meta in entries() {
            if let (Some(v), Some(lo), Some(hi)) =
                (meta.default.as_i64(), meta.min_i64, meta.max_i64)
            {
                assert!((lo..=hi).contains(&v), "{}", meta.key);
            }
            if let (Some(v), Some(lo), Some(hi)) =
                (meta.default.as_f64(), meta.min_f64, meta.max_f64)
            {
                assert!(v >= lo && v <= hi, "{}", meta.key);
            }
        }
    }

    #[test]
    fn meta_eval_alarms_group_not_hidden_on_settings_page() {
        let lower = EVAL_GROUP.to_lowercase();
        for word in [
            "steer",
            "embedding",
            "weasel",
            "analysis",
            "lesson",
            "judge",
        ] {
            assert!(!lower.contains(word));
        }
    }

    #[test]
    fn meta_eval_alarms_has_eleven_keys_in_group_evaluation() {
        let all = entries();
        assert_eq!(all.len(), 21);
        assert!(all.iter().all(|m| m.group == EVAL_GROUP));
    }

    #[test]
    fn meta_eval_alarms_label_band_registers_disabled_by_default_within_bounds() {
        use super::{
            EVAL_LABEL_BAND_CORRECTED_MAX, EVAL_LABEL_BAND_CORRECTED_MIN, EVAL_LABEL_BAND_ENABLED,
        };

        let all = entries();
        let Some(enabled) = all.iter().find(|m| m.key == EVAL_LABEL_BAND_ENABLED) else {
            panic!("{EVAL_LABEL_BAND_ENABLED} must be registered");
        };
        assert_eq!(enabled.default, serde_json::Value::Bool(false));

        let Some(min) = all.iter().find(|m| m.key == EVAL_LABEL_BAND_CORRECTED_MIN) else {
            panic!("{EVAL_LABEL_BAND_CORRECTED_MIN} must be registered");
        };
        assert_eq!(min.default.as_f64(), Some(0.32));

        let Some(max) = all.iter().find(|m| m.key == EVAL_LABEL_BAND_CORRECTED_MAX) else {
            panic!("{EVAL_LABEL_BAND_CORRECTED_MAX} must be registered");
        };
        assert_eq!(max.default.as_f64(), Some(0.49));
        assert!(min.default.as_f64() < max.default.as_f64());
    }

    #[test]
    fn meta_eval_alarms_promotion_alarms_register_enabled_by_default_within_bounds() {
        use super::{
            EVAL_METRIC_DROP_ENABLED, EVAL_METRIC_DROP_MIN_N, EVAL_SHADOW_DISAGREEMENT_ENABLED,
            EVAL_SHADOW_DISAGREEMENT_MIN_KAPPA, EVAL_SHADOW_DISAGREEMENT_MIN_N,
        };

        let all = entries();
        for key in [EVAL_SHADOW_DISAGREEMENT_ENABLED, EVAL_METRIC_DROP_ENABLED] {
            let meta = all
                .iter()
                .find(|m| m.key == key)
                .unwrap_or_else(|| panic!("{key} must be registered"));
            assert_eq!(meta.default, serde_json::Value::Bool(true));
        }
        let min_n = all
            .iter()
            .find(|m| m.key == EVAL_SHADOW_DISAGREEMENT_MIN_N)
            .unwrap_or_else(|| panic!("{EVAL_SHADOW_DISAGREEMENT_MIN_N} must be registered"));
        assert_eq!(min_n.default.as_i64(), Some(50));
        let min_kappa = all
            .iter()
            .find(|m| m.key == EVAL_SHADOW_DISAGREEMENT_MIN_KAPPA)
            .unwrap_or_else(|| panic!("{EVAL_SHADOW_DISAGREEMENT_MIN_KAPPA} must be registered"));
        assert_eq!(min_kappa.default.as_f64(), Some(0.80));
        let metric_min_n = all
            .iter()
            .find(|m| m.key == EVAL_METRIC_DROP_MIN_N)
            .unwrap_or_else(|| panic!("{EVAL_METRIC_DROP_MIN_N} must be registered"));
        assert_eq!(metric_min_n.default.as_i64(), Some(20));
    }
}
