use super::{SettingApplies, SettingKind, SettingMeta, SettingScope};

pub const HOLDOUT_PERCENT_KEY: &str = "eval.labels.holdout_percent";
pub const RELAY_ENABLED_KEY: &str = "eval.relay_labels_enabled";

fn meta(
    key: &'static str,
    kind: SettingKind,
    default: serde_json::Value,
    label: &'static str,
    description: &'static str,
) -> SettingMeta {
    let bounded = kind == SettingKind::Int;
    SettingMeta {
        key,
        group: "Evaluation",
        kind,
        scope: SettingScope::Global,
        applies: SettingApplies::NextMessage,
        default,
        label,
        description,
        min_i64: bounded.then_some(0),
        max_i64: bounded.then_some(100),
        min_f64: None,
        max_f64: None,
        max_len: None,
        enum_variants: None,
    }
}

pub fn entries() -> Vec<SettingMeta> {
    vec![
        meta(
            HOLDOUT_PERCENT_KEY,
            SettingKind::Int,
            serde_json::Value::from(DEFAULT_HOLDOUT_PERCENT),
            "Holdout share of newly labelled turns (%)",
            "The share of turns, labelled for the first time by a gold or relayed label, that \
             are assigned to the holdout split by a hash of the turn id. A turn's split never \
             changes after its first label.",
        ),
        meta(
            RELAY_ENABLED_KEY,
            SettingKind::Bool,
            serde_json::Value::Bool(true),
            "Accept relayed labels",
            "Lets a session record the operator's quoted words about a turn or session as a \
             relayed label (never gold). Off answers relay_label with a refusal naming this \
             setting.",
        ),
    ]
}

pub const DEFAULT_HOLDOUT_PERCENT: i64 = 30;
