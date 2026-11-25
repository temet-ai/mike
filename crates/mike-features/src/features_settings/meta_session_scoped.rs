use super::sections::weasel_intercept::DEFAULT_CORRECTION_PROMPT;
use crate::settings::{SettingApplies, SettingKind, SettingMeta, SettingScope};

pub fn entries() -> Vec<SettingMeta> {
    let mut v = session_toggle_entries();
    v.extend(read_dedup_entries());
    v
}

fn session_toggle_entries() -> Vec<SettingMeta> {
    vec![
        SettingMeta {
            key: "reminders.inject_lessons",
            group: "Reminders",
            kind: SettingKind::Bool,
            scope: SettingScope::Session,
            applies: SettingApplies::NewSessions,
            default: serde_json::Value::Bool(true),
            label: "Inject lessons",
            description: "Whether lesson reminders are injected into subsequent turns of a session.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "reminders.max_lessons_to_inject",
            group: "Reminders",
            kind: SettingKind::Int,
            scope: SettingScope::Session,
            applies: SettingApplies::NewSessions,
            default: serde_json::Value::Number(serde_json::Number::from(5u64)),
            label: "Max lessons to inject",
            description: "Maximum number of lessons injected per reminder turn.",
            min_i64: Some(0),
            max_i64: Some(50),
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "weasel_intercept.forbidden_words",
            group: "Weasel intercept",
            kind: SettingKind::StringList,
            scope: SettingScope::Session,
            applies: SettingApplies::NewSessions,
            default: serde_json::Value::Array(vec![]),
            label: "Forbidden words",
            description: "List of weasel words that trigger the intercept pipeline. Empty means the compiled default list is used.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: Some(200),
            enum_variants: None,
        },
        SettingMeta {
            key: "weasel_intercept.correction_prompt",
            group: "Weasel intercept",
            kind: SettingKind::Text,
            scope: SettingScope::Session,
            applies: SettingApplies::NewSessions,
            default: serde_json::Value::String(DEFAULT_CORRECTION_PROMPT.to_owned()),
            label: "Correction prompt",
            description: "Prompt injected as a user turn when a flagged phrase is detected. {word} is replaced with the phrase that was flagged. A prompt cannot be blank; Reset to default restores the built-in one.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: Some(4096),
            enum_variants: None,
        },
    ]
}

fn read_dedup_entries() -> Vec<SettingMeta> {
    vec![SettingMeta {
        key: "compression_dedup.enabled",
        group: "Read dedup",
        kind: SettingKind::Bool,
        scope: SettingScope::Session,
        applies: SettingApplies::NewSessions,
        default: serde_json::Value::Bool(false),
        label: "Rewrite duplicate reads",
        description: "Whether a duplicate-content Read/cat tool_result is replaced with the G1 stub before forwarding. When off, duplicates are still measured (shadow stats unaffected) but forwarded byte-identical.",
        min_i64: None,
        max_i64: None,
        min_f64: None,
        max_f64: None,
        max_len: None,
        enum_variants: None,
    }]
}
