use super::{SettingApplies, SettingKind, SettingMeta, SettingScope};

const GROUP: &str = "Knowledge embedding";

pub(crate) const INTERVAL_SECS: &str = "knowledge_embedding.interval_secs";
pub(crate) const BUDGET_SECS: &str = "knowledge_embedding.budget_secs";
pub(crate) const BATCH_SOURCES: &str = "knowledge_embedding.batch_sources";
pub(crate) const CONCURRENCY: &str = "knowledge_embedding.concurrency";
pub(crate) const CHUNKS_PER_CALL: &str = "knowledge_embedding.chunks_per_call";

pub(crate) const DEFAULT_INTERVAL_SECS: u64 = 30;
pub(crate) const DEFAULT_BUDGET_SECS: u64 = 10;
pub(crate) const DEFAULT_BATCH_SOURCES: i64 = 16;
pub(crate) const DEFAULT_CONCURRENCY: usize = 1;
pub(crate) const DEFAULT_CHUNKS_PER_CALL: usize = 16;

pub(super) fn entries() -> Vec<SettingMeta> {
    vec![
        int(
            INTERVAL_SECS,
            DEFAULT_INTERVAL_SECS as i64,
            (5, 3_600),
            "Seconds between embedding sweeps",
            "How long the knowledge embedder rests between sweeps. Lower it to embed a backlog faster; the sweep only works while sources are waiting.",
        ),
        int(
            BUDGET_SECS,
            DEFAULT_BUDGET_SECS as i64,
            (1, 600),
            "Seconds of work per sweep",
            "How long one sweep keeps embedding before it stops and rests. Raising it, or setting it near the interval, makes the embedder work almost continuously on a backlog.",
        ),
        int(
            BATCH_SOURCES,
            DEFAULT_BATCH_SOURCES,
            (16, 256),
            "Sources read per round",
            "How many waiting sources the embedder claims at once.",
        ),
        int(
            CONCURRENCY,
            DEFAULT_CONCURRENCY as i64,
            (1, 16),
            "Sources embedded at once",
            "How many sources are sent to the embedding model in parallel. Higher is faster until the embedding server is the limit; on a shared machine raise it a step at a time.",
        ),
        int(
            CHUNKS_PER_CALL,
            DEFAULT_CHUNKS_PER_CALL as i64,
            (1, 64),
            "Text chunks per embedding call",
            "How many chunks of one source go to the embedding model in a single request.",
        ),
    ]
}

fn int(
    key: &'static str,
    default: i64,
    (min, max): (i64, i64),
    label: &'static str,
    description: &'static str,
) -> SettingMeta {
    SettingMeta {
        key,
        group: GROUP,
        kind: SettingKind::Int,
        scope: SettingScope::Global,
        applies: SettingApplies::NextCycle,
        default: serde_json::Value::from(default),
        label,
        description,
        min_i64: Some(min),
        max_i64: Some(max),
        min_f64: None,
        max_f64: None,
        max_len: None,
        enum_variants: None,
    }
}
