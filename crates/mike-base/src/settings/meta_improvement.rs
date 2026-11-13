use super::{SettingApplies, SettingKind, SettingMeta, SettingScope};

pub fn entries() -> Vec<SettingMeta> {
    let mut entries = vec![
        SettingMeta {
            key: "judge_lessons.model",
            group: "Lesson proposer",
            kind: SettingKind::String,
            scope: SettingScope::Global,
            applies: SettingApplies::NextCycle,
            default: serde_json::Value::String(String::new()),
            label: "Model",
            description: "OpenRouter model the lesson proposer sends each judged correction \
                to, or codex/<model> to run it through the local Codex login instead. It must \
                support structured output. Blank falls back to the session judge's model.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: Some(256),
            enum_variants: None,
        },
        SettingMeta {
            key: "judge_lessons.min_confidence",
            group: "Lesson proposer",
            kind: SettingKind::Float,
            scope: SettingScope::Global,
            applies: SettingApplies::NextCycle,
            default: serde_json::json!(0.7),
            label: "Minimum verdict confidence",
            description: "A corrected verdict below this confidence is never turned into a \
                lesson candidate.",
            min_i64: None,
            max_i64: None,
            min_f64: Some(0.0),
            max_f64: Some(1.0),
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "judge_lessons.max_per_day",
            group: "Lesson proposer",
            kind: SettingKind::Int,
            scope: SettingScope::Global,
            applies: SettingApplies::NextCycle,
            default: serde_json::Value::Number(serde_json::Number::from(40u64)),
            label: "Corrections examined per day",
            description: "How many corrected verdicts the proposer will examine across all of \
                today's ticks. Each one it examines is at most one OpenRouter call.",
            min_i64: Some(0),
            max_i64: Some(1000),
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "judge_lessons.auto_promote_min_evidence",
            group: "Lesson proposer",
            kind: SettingKind::Int,
            scope: SettingScope::Global,
            applies: SettingApplies::NextCycle,
            default: serde_json::Value::Number(serde_json::Number::from(2u64)),
            label: "Corrections needed to inject automatically",
            description: "How many corrections must back a candidate before the proposer \
                injects it itself. 0 means every candidate waits for the operator; 1 means \
                every candidate is injected immediately.",
            min_i64: Some(0),
            max_i64: Some(100),
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: crate::lesson_shape::MAX_CHARS_SETTING,
            group: "Lesson proposer",
            kind: SettingKind::Int,
            scope: SettingScope::Global,
            applies: SettingApplies::NextCycle,
            default: serde_json::Value::Number(serde_json::Number::from(500u64)),
            label: "Longest lesson kept as written",
            description: "A lesson longer than this many characters, or one that opens with a \
                status word or names a session uuid or commit sha as its subject, is accepted \
                but held at the raw stage and not injected until the proposer rewrites it as \
                one durable rule. The same limit applies to the rewrites.",
            min_i64: Some(100),
            max_i64: Some(5000),
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
    ];
    entries.push(maintenance_budget_entry());
    entries.push(skill_cap_entry());
    entries.extend(quiet_budget_entries());
    entries
}

fn maintenance_budget_entry() -> SettingMeta {
    SettingMeta {
        key: "judge_lessons.maintenance_max_per_day",
        group: "Lesson proposer",
        kind: SettingKind::Int,
        scope: SettingScope::Global,
        applies: SettingApplies::NextCycle,
        default: serde_json::Value::Number(serde_json::Number::from(400u64)),
        label: "Lesson maintenance calls per day",
        description: "How many model calls the lesson maintenance passes may make across \
            today's ticks: rewriting over-long lessons into rules and deciding where each \
            lesson belongs. These calls are counted apart from the corrections allowance, \
            so a large backlog of maintenance never uses up the calls for new corrections.",
        min_i64: Some(0),
        max_i64: Some(5000),
        min_f64: None,
        max_f64: None,
        max_len: None,
        enum_variants: None,
    }
}

fn skill_cap_entry() -> SettingMeta {
    SettingMeta {
        key: "lessons.skills.max_per_project",
        group: "Lesson proposer",
        kind: SettingKind::Int,
        scope: SettingScope::Global,
        applies: SettingApplies::NextCycle,
        default: serde_json::Value::Number(serde_json::Number::from(12u64)),
        label: "Most learned skills per project",
        description: "The most skills mike writes into one project. When a new skill would \
            go past this number, the promoter first retires the learned skill with the \
            least evidence of use (the fewest loads in the last 30 days), and holds the \
            new skill back if every existing one is younger than a week.",
        min_i64: Some(1),
        max_i64: Some(100),
        min_f64: None,
        max_f64: None,
        max_len: None,
        enum_variants: None,
    }
}

fn quiet_budget_entries() -> Vec<SettingMeta> {
    [
        ("judge_lessons.quiet_max_per_day", "Quiet-hour daily cap", 0, 0, 1000,
            "Total UTC-day call allowance while the configured quiet window has no recent primary traffic. Zero disables the boost; uses the same durable daily counter."),
        ("judge_lessons.quiet_start_hour_utc", "Quiet window starts (UTC hour)", 22, 0, 23,
            "Inclusive UTC hour for quiet catch-up. A window may cross midnight; equal start and end disables it."),
        ("judge_lessons.quiet_end_hour_utc", "Quiet window ends (UTC hour)", 4, 0, 23,
            "Exclusive UTC hour for quiet catch-up. Default 22–04 UTC is midnight–06:00 in Johannesburg."),
        ("judge_lessons.quiet_idle_secs", "Primary traffic quiet period (seconds)", 300, 60, 86400,
            "Minimum time without primary request activity before the quiet-hour daily cap applies. Activity restores the normal cap."),
    ].into_iter().map(|(key, label, default, min, max, description)| SettingMeta {
        key, label, description, group: "Lesson proposer", kind: SettingKind::Int,
        scope: SettingScope::Global, applies: SettingApplies::NextCycle,
        default: serde_json::json!(default), min_i64: Some(min), max_i64: Some(max),
        min_f64: None, max_f64: None, max_len: None, enum_variants: None,
    }).collect()
}
