use serde_json::Value;

pub const EFFICIENCY_MIN_INTERVAL_SECS: u64 = 1_800;
pub const DEFAULT_ADVICE_MEMORY_HOURS: u64 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Rollout {
    #[default]
    Live,
    Shadow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SteererClass {
    #[default]
    Correction,
    Efficiency,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Requirement {
    Binary(String),
    ToolNameContains(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RolloutParams {
    pub rollout: Rollout,
    pub quiescence_after_secs: Option<i64>,
    pub quiescence_question: Option<String>,
    pub class: SteererClass,
    pub min_interval_secs: u64,
    pub advice_memory_hours: u64,
    pub requires: Vec<Requirement>,
}

fn present<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.get(key).filter(|value| !value.is_null())
}

fn parse_rollout(v: &Value) -> Rollout {
    match present(v, "rollout").and_then(Value::as_str) {
        Some("shadow") => Rollout::Shadow,
        _ => Rollout::Live,
    }
}

fn parse_class(v: &Value) -> SteererClass {
    match present(v, "class").and_then(Value::as_str) {
        Some("efficiency") => SteererClass::Efficiency,
        _ => SteererClass::Correction,
    }
}

fn parse_requirement(item: &Value) -> Option<Requirement> {
    if let Some(binary) = item.get("binary").and_then(Value::as_str) {
        return Some(Requirement::Binary(binary.to_owned()));
    }
    item.get("tool_name_contains")
        .and_then(Value::as_str)
        .map(|text| Requirement::ToolNameContains(text.to_owned()))
}

fn parse_requires(v: &Value) -> Vec<Requirement> {
    present(v, "requires")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(parse_requirement).collect())
        .unwrap_or_default()
}

impl RolloutParams {
    pub fn from_json(v: &Value) -> Self {
        let class = parse_class(v);
        let default_interval = match class {
            SteererClass::Efficiency => EFFICIENCY_MIN_INTERVAL_SECS,
            SteererClass::Correction => 0,
        };
        Self {
            rollout: parse_rollout(v),
            quiescence_after_secs: present(v, "quiescence_after_secs").and_then(Value::as_i64),
            quiescence_question: present(v, "quiescence_question")
                .and_then(Value::as_str)
                .map(str::to_owned),
            class,
            min_interval_secs: present(v, "min_interval_secs")
                .and_then(Value::as_u64)
                .unwrap_or(default_interval),
            advice_memory_hours: present(v, "advice_memory_hours")
                .and_then(Value::as_u64)
                .unwrap_or(DEFAULT_ADVICE_MEMORY_HOURS),
            requires: parse_requires(v),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EFFICIENCY_MIN_INTERVAL_SECS, Requirement, Rollout, RolloutParams, SteererClass};
    use serde_json::json;

    #[test]
    fn steerer_params_rollout_absent_defaults_live() {
        assert_eq!(RolloutParams::from_json(&json!({})).rollout, Rollout::Live);
    }

    #[test]
    fn steerer_params_rollout_shadow_parsed() {
        let p = RolloutParams::from_json(&json!({"rollout": "shadow"}));
        assert_eq!(p.rollout, Rollout::Shadow);
    }

    #[test]
    fn steerer_params_rollout_unknown_value_defaults_live() {
        let p = RolloutParams::from_json(&json!({"rollout": "canary"}));
        assert_eq!(p.rollout, Rollout::Live);
    }

    #[test]
    fn steerer_params_quiescence_after_secs_parsed() {
        let p = RolloutParams::from_json(
            &json!({"quiescence_after_secs": 90, "quiescence_question": "unbacked_wait"}),
        );
        assert_eq!(p.quiescence_after_secs, Some(90));
        assert_eq!(p.quiescence_question.as_deref(), Some("unbacked_wait"));
    }

    #[test]
    fn steerer_params_quiescence_null_treated_absent() {
        let p = RolloutParams::from_json(&json!({"quiescence_after_secs": null}));
        assert_eq!(p.quiescence_after_secs, None);
    }

    #[test]
    fn efficiency_class_defaults_to_a_thirty_minute_interval() {
        let p = RolloutParams::from_json(&json!({"class": "efficiency"}));
        assert_eq!(p.class, SteererClass::Efficiency);
        assert_eq!(p.min_interval_secs, EFFICIENCY_MIN_INTERVAL_SECS);
        assert_eq!(RolloutParams::from_json(&json!({})).min_interval_secs, 0);
    }

    #[test]
    fn requires_parses_binaries_and_tool_names() {
        let p = RolloutParams::from_json(&json!({"requires": [
            {"binary": "ast-grep"}, {"tool_name_contains": "lsp"}, {"other": 1}
        ]}));
        assert_eq!(
            p.requires,
            vec![
                Requirement::Binary("ast-grep".to_owned()),
                Requirement::ToolNameContains("lsp".to_owned())
            ]
        );
    }
}
