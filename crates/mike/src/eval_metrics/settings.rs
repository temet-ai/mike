use std::collections::HashMap;

use serde_json::Value;

use crate::settings::meta_eval_alarms as keys;
use crate::settings::{SETTINGS_META, SettingKind, SettingMeta};
use crate::state::AppState;

#[derive(Debug, Clone, PartialEq)]
pub struct EvalSettings {
    pub rollup_enabled: bool,
    pub rollup_hour_utc: u32,
    pub mix_min_turns: i64,
    pub mix_min_tvd: f64,
    pub mix_min_chi2: f64,
    pub backlog_min_turns: i64,
    pub stalled_min_waiting: i64,
    pub stalled_hours: i64,
    pub refusals_min_share: f64,
    pub refusals_min_refused: i64,
    pub gate_grace_hours: i64,
    pub call_loop_min_calls: i64,
    pub call_loop_max_calls_per_row: f64,
    pub band_enabled: bool,
    pub band_min: f64,
    pub band_max: f64,
    pub shadow_disagreement_enabled: bool,
    pub shadow_disagreement_min_n: i64,
    pub shadow_disagreement_min_kappa: f64,
    pub metric_drop_enabled: bool,
    pub metric_drop_min_n: i64,
}

fn within_bounds(meta: &SettingMeta, value: &Value) -> bool {
    match meta.kind {
        SettingKind::Bool => value.is_boolean(),
        SettingKind::Int => value.as_i64().is_some_and(|v| {
            meta.min_i64.is_none_or(|lo| v >= lo) && meta.max_i64.is_none_or(|hi| v <= hi)
        }),
        SettingKind::Float => value.as_f64().is_some_and(|v| {
            meta.min_f64.is_none_or(|lo| v >= lo) && meta.max_f64.is_none_or(|hi| v <= hi)
        }),
        _ => false,
    }
}

pub(crate) fn resolved_value(meta: &SettingMeta, stored: Option<&Value>) -> Value {
    match stored {
        Some(value) if within_bounds(meta, value) => value.clone(),
        Some(value) => {
            tracing::warn!(key = meta.key, stored = %value, "eval_metrics: stored setting out of bounds — using the default");
            meta.default.clone()
        }
        None => meta.default.clone(),
    }
}

struct Lookup<'a> {
    rows: HashMap<&'a str, &'a Value>,
}

impl Lookup<'_> {
    fn value(&self, key: &str) -> Value {
        let Some(meta) = SETTINGS_META.iter().find(|m| m.key == key) else {
            return Value::Null;
        };
        resolved_value(meta, self.rows.get(key).copied())
    }

    fn int(&self, key: &str) -> i64 {
        self.value(key).as_i64().unwrap_or_default()
    }

    fn float(&self, key: &str) -> f64 {
        self.value(key).as_f64().unwrap_or_default()
    }
}

fn rollup_and_mix_settings(l: &Lookup<'_>) -> (bool, u32, i64, f64, f64) {
    (
        l.value(keys::EVAL_ROLLUP_ENABLED).as_bool().unwrap_or(true),
        u32::try_from(l.int(keys::EVAL_ROLLUP_HOUR_UTC)).unwrap_or(3),
        l.int(keys::EVAL_LABEL_MIX_MIN_TURNS),
        l.float(keys::EVAL_LABEL_MIX_MIN_TVD),
        l.float(keys::EVAL_LABEL_MIX_MIN_CHI2),
    )
}

pub fn resolve_eval_settings(rows: &[(String, Value)]) -> EvalSettings {
    let lookup = Lookup {
        rows: rows
            .iter()
            .filter(|(k, _)| k.starts_with("eval.rollup.") || k.starts_with("eval.alarm."))
            .map(|(k, v)| (k.as_str(), v))
            .collect(),
    };
    let (rollup_enabled, rollup_hour_utc, mix_min_turns, mix_min_tvd, mix_min_chi2) =
        rollup_and_mix_settings(&lookup);
    EvalSettings {
        rollup_enabled,
        rollup_hour_utc,
        mix_min_turns,
        mix_min_tvd,
        mix_min_chi2,
        backlog_min_turns: lookup.int(keys::EVAL_BACKLOG_MIN_TURNS),
        stalled_min_waiting: lookup.int(keys::EVAL_STALLED_MIN_WAITING),
        stalled_hours: lookup.int(keys::EVAL_STALLED_HOURS),
        refusals_min_share: lookup.float(keys::EVAL_REFUSALS_MIN_SHARE),
        refusals_min_refused: lookup.int(keys::EVAL_REFUSALS_MIN_REFUSED),
        gate_grace_hours: lookup.int(keys::EVAL_GATE_GRACE_HOURS),
        call_loop_min_calls: lookup.int(keys::EVAL_CALL_LOOP_MIN_CALLS),
        call_loop_max_calls_per_row: lookup.float(keys::EVAL_CALL_LOOP_MAX_PER_ROW),
        band_enabled: lookup
            .value(keys::EVAL_LABEL_BAND_ENABLED)
            .as_bool()
            .unwrap_or(false),
        band_min: lookup.float(keys::EVAL_LABEL_BAND_CORRECTED_MIN),
        band_max: lookup.float(keys::EVAL_LABEL_BAND_CORRECTED_MAX),
        shadow_disagreement_enabled: lookup
            .value(keys::EVAL_SHADOW_DISAGREEMENT_ENABLED)
            .as_bool()
            .unwrap_or(true),
        shadow_disagreement_min_n: lookup.int(keys::EVAL_SHADOW_DISAGREEMENT_MIN_N),
        shadow_disagreement_min_kappa: lookup.float(keys::EVAL_SHADOW_DISAGREEMENT_MIN_KAPPA),
        metric_drop_enabled: lookup
            .value(keys::EVAL_METRIC_DROP_ENABLED)
            .as_bool()
            .unwrap_or(true),
        metric_drop_min_n: lookup.int(keys::EVAL_METRIC_DROP_MIN_N),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RoleInputs {
    pub judge_toggle: bool,
    pub judge_config_enabled: bool,
    pub session_judge_toggle: bool,
    pub steerer_toggle: bool,
}

pub fn expected_roles_from(inputs: RoleInputs) -> Vec<&'static str> {
    let mut roles = Vec::new();
    if inputs.judge_toggle && inputs.judge_config_enabled {
        roles.push("turn_judge");
    }
    if inputs.session_judge_toggle {
        roles.push("session_judge");
    }
    if inputs.steerer_toggle {
        roles.push("steerer");
    }
    roles
}

pub(crate) fn expected_roles(state: &AppState) -> Vec<&'static str> {
    let toggles = state.feature_toggle_overlay.load();
    expected_roles_from(RoleInputs {
        judge_toggle: toggles.judge_worker,
        judge_config_enabled: state.config().steerer.judge.enabled,
        session_judge_toggle: toggles.session_judge,
        steerer_toggle: toggles.steerer,
    })
}
