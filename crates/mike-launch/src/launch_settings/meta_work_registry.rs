use crate::settings::{SettingApplies, SettingKind, SettingMeta, SettingScope};

pub const WAKE_MODE_KEY: &str = "work_registry.wake_mode";

pub fn entries() -> Vec<SettingMeta> {
    vec![SettingMeta {
        key: WAKE_MODE_KEY,
        group: "Work registry",
        kind: SettingKind::Enum,
        scope: SettingScope::Global,
        applies: SettingApplies::NextCycle,
        default: serde_json::Value::String("log".to_owned()),
        label: "Wake mode",
        description: "What the work-registry reconciler does when a session needs attention \
            (a result nobody received, a background job whose owner went quiet, an agent \
            that has landed its work and is only polling, or a main thread idle with open \
            work). 'log' records each finding once in the ledger and the proxy log. \
            'deliver' also sends it to that session's mailbox, which wakes it through the \
            normal delivery path. Read on every reconciler pass, once a minute.",
        min_i64: None,
        max_i64: None,
        min_f64: None,
        max_f64: None,
        max_len: None,
        enum_variants: Some(&["log", "deliver"]),
    }]
}
