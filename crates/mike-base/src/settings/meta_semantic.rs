use super::{SettingApplies, SettingKind, SettingMeta, SettingScope};

pub const ALLOW_REMOTE_EMBEDDING: &str = "semantic.allow_remote_embedding";
pub const SWEEP_INTERVAL_SECS: &str = "semantic.sweep_interval_secs";
pub const BATCH_TURNS: &str = "semantic.batch_turns";
pub const ENDING_CHARS: &str = "semantic.ending_chars";
pub const INLINE_TIMEOUT_MS: &str = "semantic.inline_timeout_ms";
pub const FLOOR_QUANTILE: &str = "semantic.floor_quantile";
pub const FLOOR_SAMPLE_TURNS: &str = "semantic.floor_sample_turns";
pub const RETRIEVAL_SCOPE: &str = "semantic.retrieval_scope";
pub const RETRIEVAL_K: &str = "semantic.retrieval_k";
pub const RELAPSE_EVIDENCE: &str = "semantic.relapse_evidence";
pub const TAGGER_MIN_SCORE: &str = "semantic.tagger_min_score";
pub const CLUSTER_FLOOR_QUANTILE: &str = "semantic.cluster_floor_quantile";
pub const CLUSTER_MIN_MEMBERS: &str = "semantic.cluster_min_members";
pub const CLUSTER_MIN_SESSIONS: &str = "semantic.cluster_min_sessions";
pub const DRIFT_Z: &str = "semantic.drift_z";
pub const DRIFT_MIN_CURRENT: &str = "semantic.drift_min_current";

pub const SEMANTIC_GROUP: &str = "Semantic context";

fn base(
    key: &'static str,
    kind: SettingKind,
    default: serde_json::Value,
    text: (&'static str, &'static str),
) -> SettingMeta {
    SettingMeta {
        key,
        group: SEMANTIC_GROUP,
        kind,
        scope: SettingScope::Global,
        applies: SettingApplies::NextCycle,
        default,
        label: text.0,
        description: text.1,
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
        min_i64: Some(bounds.0),
        max_i64: Some(bounds.1),
        ..base(
            key,
            SettingKind::Int,
            serde_json::Value::from(default),
            text,
        )
    }
}

fn float(
    key: &'static str,
    default: f64,
    bounds: (f64, f64),
    text: (&'static str, &'static str),
) -> SettingMeta {
    SettingMeta {
        min_f64: Some(bounds.0),
        max_f64: Some(bounds.1),
        ..base(
            key,
            SettingKind::Float,
            serde_json::Value::from(default),
            text,
        )
    }
}

fn choice(
    key: &'static str,
    default: &'static str,
    variants: &'static [&'static str],
    text: (&'static str, &'static str),
) -> SettingMeta {
    SettingMeta {
        enum_variants: Some(variants),
        ..base(
            key,
            SettingKind::Enum,
            serde_json::Value::from(default),
            text,
        )
    }
}

fn worker_entries() -> Vec<SettingMeta> {
    vec![
        base(
            ALLOW_REMOTE_EMBEDDING,
            SettingKind::Bool,
            serde_json::Value::Bool(false),
            (
                "Allow a remote embedding server",
                "Off: every semantic embed call is refused unless the embedding URL is loopback, so no text leaves the machine. On: text is sent to whatever embedding.url names.",
            ),
        ),
        int(
            SWEEP_INTERVAL_SECS,
            300,
            (60, 3_600),
            (
                "Semantic worker interval (s)",
                "How often the semantic worker builds items, tags corrections and checks the embedder.",
            ),
        ),
        int(
            BATCH_TURNS,
            64,
            (8, 256),
            (
                "Turns per build batch",
                "Closed turns embedded per tick, in one embedding call.",
            ),
        ),
        int(
            ENDING_CHARS,
            1_024,
            (256, 4_096),
            (
                "Turn-ending characters",
                "How much of a turn's non-sidecar ending is embedded, cut back to whitespace.",
            ),
        ),
        int(
            INLINE_TIMEOUT_MS,
            5_000,
            (500, 30_000),
            (
                "Inline embed timeout (ms)",
                "The most a judge or guard waits for one turn's items to be built before it records embed_timeout and moves on.",
            ),
        ),
        float(
            FLOOR_QUANTILE,
            0.999,
            (0.99, 0.9999),
            (
                "Chance floor quantile",
                "Similarity below this quantile of unrelated cross-session pairs counts as chance.",
            ),
        ),
        int(
            FLOOR_SAMPLE_TURNS,
            300,
            (100, 1_000),
            (
                "Floor sample size",
                "Items sampled on each side when a chance floor is computed.",
            ),
        ),
    ]
}

fn use_entries() -> Vec<SettingMeta> {
    vec![
        choice(
            RETRIEVAL_SCOPE,
            "operator",
            &["operator", "project", "session"],
            (
                "Retrieval scope",
                "Where similar earlier words are looked for: every project (operator), the turn's project, or the turn's session.",
            ),
        ),
        int(
            RETRIEVAL_K,
            3,
            (1, 10),
            (
                "Neighbours per query",
                "How many similar items a consumer receives.",
            ),
        ),
        choice(
            RELAPSE_EVIDENCE,
            "record",
            &["off", "record", "input"],
            (
                "Relapse evidence",
                "record writes similar earlier corrections into the guard's evaluation record only; input also adds the best one to the guard's prompt, which is a new bundle.",
            ),
        ),
        float(
            TAGGER_MIN_SCORE,
            0.05,
            (-0.2, 0.3),
            (
                "Tagger minimum score",
                "A correction is tagged to the nearest pattern only when its similarity beats the chance floor by this much.",
            ),
        ),
        float(
            CLUSTER_FLOOR_QUANTILE,
            0.99,
            (0.9, 0.999),
            (
                "Grouping floor quantile",
                "Chance quantile an edge must beat for two untagged corrections to be grouped.",
            ),
        ),
        int(
            CLUSTER_MIN_MEMBERS,
            4,
            (3, 20),
            (
                "Group minimum members",
                "Corrections needed before a group is proposed as a new pattern.",
            ),
        ),
        int(
            CLUSTER_MIN_SESSIONS,
            3,
            (2, 10),
            (
                "Group minimum sessions",
                "Distinct sessions needed before a group is proposed.",
            ),
        ),
        float(
            DRIFT_Z,
            3.0,
            (2.0, 6.0),
            (
                "Drift alarm z",
                "Daily embedding drift at or above this z raises the embedding_drift alarm.",
            ),
        ),
        int(
            DRIFT_MIN_CURRENT,
            30,
            (5, 200),
            (
                "Drift minimum items",
                "Items needed in the current week before drift is scored; corrections use a third of it.",
            ),
        ),
    ]
}

pub fn entries() -> Vec<SettingMeta> {
    let mut v = worker_entries();
    v.extend(use_entries());
    v
}
