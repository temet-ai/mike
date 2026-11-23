use crate::settings::{SettingApplies, SettingKind, SettingMeta, SettingScope};

const GROUP: &str = "Knowledge consolidation";

pub const INTERVAL_SECS: &str = "knowledge_consolidation.interval_secs";
pub const ITEMS_PER_TICK: &str = "knowledge_consolidation.items_per_tick";
pub const SUBJECTS_PER_TICK: &str = "knowledge_consolidation.subjects_per_tick";
pub const SETTLE_SECS: &str = "knowledge_consolidation.settle_secs";
pub const MAX_JUDGEMENTS_PER_DAY: &str = "knowledge_consolidation.max_judgements_per_day";
pub const MODEL: &str = "knowledge_consolidation.model";
pub const GROUPING_SIMILARITY: &str = "knowledge_consolidation.grouping_similarity";
pub const EDIT_FILES: &str = "knowledge_consolidation.edit_files";

pub const DEFAULT_INTERVAL_SECS: i64 = 60;
pub const DEFAULT_ITEMS_PER_TICK: i64 = 8;
pub const DEFAULT_SUBJECTS_PER_TICK: i64 = 4;
pub const DEFAULT_SETTLE_SECS: i64 = 120;
pub const DEFAULT_MAX_JUDGEMENTS_PER_DAY: i64 = 400;
pub const DEFAULT_GROUPING_SIMILARITY: f64 = 0.70;

pub fn entries() -> Vec<SettingMeta> {
    vec![
        int(
            INTERVAL_SECS,
            DEFAULT_INTERVAL_SECS,
            (10, 3_600),
            "Seconds between consolidation passes",
            "How long the knowledge consolidation worker rests between passes. A pass splits the items that changed, groups their statements and judges the subjects that settled. It only calls a model when something is waiting.",
        ),
        int(
            ITEMS_PER_TICK,
            DEFAULT_ITEMS_PER_TICK,
            (1, 64),
            "Items split per pass",
            "How many changed items one pass takes. Each file is one model call; lessons and session evidence need none.",
        ),
        int(
            SUBJECTS_PER_TICK,
            DEFAULT_SUBJECTS_PER_TICK,
            (1, 32),
            "Subjects judged per pass",
            "How many settled subjects one pass judges, one model call each.",
        ),
        int(
            SETTLE_SECS,
            DEFAULT_SETTLE_SECS,
            (0, 86_400),
            "Seconds a changed subject settles before it is judged",
            "A subject is judged once nothing has joined or left it for this long, so a burst of edits is judged once.",
        ),
        int(
            MAX_JUDGEMENTS_PER_DAY,
            DEFAULT_MAX_JUDGEMENTS_PER_DAY,
            (0, 100_000),
            "Subject judgements per day",
            "The most model calls the worker spends judging subjects in one UTC day. Waiting subjects are judged the next day.",
        ),
        text(
            MODEL,
            "Consolidation model",
            "Model that judges a subject. Blank uses the standard tier of the configured LLM provider (claude sonnet by default).",
        ),
        float(
            GROUPING_SIMILARITY,
            DEFAULT_GROUPING_SIMILARITY,
            (0.5, 0.95),
            "Starting grouping similarity",
            "How close a new statement must be to a subject to join it, for a project with no recorded measure yet. The worker moves a project's value by 0.01 per run between 0.62 and 0.80 from the share of misgrouped and missed statements it finds.",
        ),
        boolean(
            EDIT_FILES,
            "Write consolidation edits to files",
            "When on, a statement found covered or contradicted in a hand-written file is removed from that file on this machine, after checking that the bytes at the recorded place still equal the passage. When off the exact edit is recorded as proposed.",
        ),
    ]
}

fn base(
    key: &'static str,
    kind: SettingKind,
    default: serde_json::Value,
    label: &'static str,
    description: &'static str,
) -> SettingMeta {
    SettingMeta {
        key,
        group: GROUP,
        kind,
        scope: SettingScope::Global,
        applies: SettingApplies::NextCycle,
        default,
        label,
        description,
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
    (min, max): (i64, i64),
    label: &'static str,
    description: &'static str,
) -> SettingMeta {
    SettingMeta {
        min_i64: Some(min),
        max_i64: Some(max),
        ..base(
            key,
            SettingKind::Int,
            serde_json::Value::from(default),
            label,
            description,
        )
    }
}

fn float(
    key: &'static str,
    default: f64,
    (min, max): (f64, f64),
    label: &'static str,
    description: &'static str,
) -> SettingMeta {
    SettingMeta {
        min_f64: Some(min),
        max_f64: Some(max),
        ..base(
            key,
            SettingKind::Float,
            serde_json::Value::from(default),
            label,
            description,
        )
    }
}

fn text(key: &'static str, label: &'static str, description: &'static str) -> SettingMeta {
    SettingMeta {
        max_len: Some(256),
        ..base(
            key,
            SettingKind::String,
            serde_json::Value::String(String::new()),
            label,
            description,
        )
    }
}

fn boolean(key: &'static str, label: &'static str, description: &'static str) -> SettingMeta {
    base(
        key,
        SettingKind::Bool,
        serde_json::Value::Bool(false),
        label,
        description,
    )
}
