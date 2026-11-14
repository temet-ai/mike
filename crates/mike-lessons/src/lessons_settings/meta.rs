use crate::settings::{SettingApplies, SettingKind, SettingMeta, SettingScope};

pub fn entries() -> Vec<SettingMeta> {
    let mut entries = vec![recall_injector_entry()];
    entries.extend(lesson_worker_entries());
    entries.extend(lesson_relations_entries());
    entries.extend(super::meta_memory_loop::entries());
    entries
}

fn recall_injector_entry() -> SettingMeta {
    SettingMeta {
        key: super::overlay::RECALL_MAX_LESSONS_PER_TOPIC_KEY,
        group: "Recall injector",
        kind: SettingKind::Int,
        scope: SettingScope::Session,
        applies: SettingApplies::NewSessions,
        default: serde_json::Value::Number(serde_json::Number::from(3u64)),
        label: "Max lessons per topic",
        description: "Maximum lessons recalled from the lesson store per topic per session.",
        min_i64: Some(0),
        max_i64: Some(20),
        min_f64: None,
        max_f64: None,
        max_len: None,
        enum_variants: None,
    }
}

fn lesson_worker_entries() -> Vec<SettingMeta> {
    vec![
        SettingMeta {
            key: "lesson_worker.dedup_threshold",
            group: "Lesson worker",
            kind: SettingKind::Float,
            scope: SettingScope::Global,
            applies: SettingApplies::OnRestart,
            default: serde_json::json!(0.85),
            label: "Dedup threshold",
            description: "Cosine similarity threshold above which two lessons are considered duplicates.",
            min_i64: None,
            max_i64: None,
            min_f64: Some(0.0),
            max_f64: Some(1.0),
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "lesson_worker.max_lessons_per_project",
            group: "Lesson worker",
            kind: SettingKind::Int,
            scope: SettingScope::Global,
            applies: SettingApplies::OnRestart,
            default: serde_json::Value::Number(serde_json::Number::from(500i64)),
            label: "Max lessons per project",
            description: "Maximum active lessons retained per project after GC.",
            min_i64: Some(10),
            max_i64: Some(10_000),
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "lesson_worker.session_consolidation_idle_secs",
            group: "Lesson worker",
            kind: SettingKind::Int,
            scope: SettingScope::Global,
            applies: SettingApplies::OnRestart,
            default: serde_json::Value::Number(serde_json::Number::from(300u64)),
            label: "Session consolidation idle (secs)",
            description: "Seconds of inactivity before a session's lessons are consolidated.",
            min_i64: Some(60),
            max_i64: Some(86400),
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "lesson_worker.nightly_consolidation_hour",
            group: "Lesson worker",
            kind: SettingKind::Int,
            scope: SettingScope::Global,
            applies: SettingApplies::OnRestart,
            default: serde_json::Value::Number(serde_json::Number::from(3u64)),
            label: "Nightly consolidation hour",
            description: "Local hour (0–23) at which the nightly lesson consolidation runs.",
            min_i64: Some(0),
            max_i64: Some(23),
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "lesson_worker.gc_interval_secs",
            group: "Lesson worker",
            kind: SettingKind::Int,
            scope: SettingScope::Global,
            applies: SettingApplies::OnRestart,
            default: serde_json::Value::Number(serde_json::Number::from(3600u64)),
            label: "GC interval (secs)",
            description: "How often the lesson GC sweeps old entries.",
            min_i64: Some(60),
            max_i64: Some(86400),
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
    ]
}

fn lesson_relations_entries() -> Vec<SettingMeta> {
    vec![
        SettingMeta {
            key: "lesson_relations.enabled",
            group: "Lesson relations",
            kind: SettingKind::Bool,
            scope: SettingScope::Global,
            applies: SettingApplies::OnRestart,
            default: serde_json::Value::Bool(true),
            label: "Lesson relations: enabled",
            description: "Enable the `mike detect-lesson-relations` detector, which proposes \
                supersedes/refines/recurs/contradicts relations between lessons and runs \
                nightly as the lesson_relations scheduled job. When the adjudicator agrees \
                with a proposed contradiction, the nightly consolidation confirms it and \
                retires the older lesson from supply as superseded; it stays searchable and \
                nothing is deleted.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "lesson_relations.proposer_model",
            group: "Lesson relations",
            kind: SettingKind::String,
            scope: SettingScope::Global,
            applies: SettingApplies::OnRestart,
            default: serde_json::Value::String("openai/gpt-5.6-luna".to_owned()),
            label: "Proposer model",
            description: "OpenRouter model, or codex/<model>, that proposes a relation from an \
                evidence bundle. Must differ from the adjudicator model — a model never \
                adjudicates its own proposal.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: Some(256),
            enum_variants: None,
        },
        SettingMeta {
            key: "lesson_relations.adjudicator_model",
            group: "Lesson relations",
            kind: SettingKind::String,
            scope: SettingScope::Global,
            applies: SettingApplies::OnRestart,
            default: serde_json::Value::String("google/gemini-3.5-flash-lite".to_owned()),
            label: "Adjudicator model",
            description: "OpenRouter model, or codex/<model>, that independently reviews the \
                proposer's claim against the same evidence. Must differ from the proposer model.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: Some(256),
            enum_variants: None,
        },
        SettingMeta {
            key: "lesson_relations.candidate_distance_threshold",
            group: "Lesson relations",
            kind: SettingKind::Float,
            scope: SettingScope::Global,
            applies: SettingApplies::OnRestart,
            default: serde_json::json!(0.2),
            label: "Candidate distance threshold",
            description: "Cosine distance below which a same-project lesson pair is nominated \
                as a relation candidate. Nomination only — kind, direction, and status always \
                come from evidence, never from this threshold alone.",
            min_i64: None,
            max_i64: None,
            min_f64: Some(0.0),
            max_f64: Some(1.0),
            max_len: None,
            enum_variants: None,
        },
    ]
}
