use crate::settings::{SettingApplies, SettingKind, SettingMeta, SettingScope};

pub const FLUSH_INTERVAL_MS_KEY: &str = "dossier.flush_interval_ms";
pub const BATCH_SESSIONS_KEY: &str = "dossier.batch_sessions";
pub const BACKSTOP_INTERVAL_SECS_KEY: &str = "dossier.backstop_interval_secs";
pub const BACKSTOP_LOOKBACK_HOURS_KEY: &str = "dossier.backstop_lookback_hours";
pub const SESSION_SETTLE_MINUTES_KEY: &str = "dossier.session_settle_minutes";
pub const ENTRIES_ENABLED_KEY: &str = "dossier.entries.enabled";

pub const DEFAULT_FLUSH_INTERVAL_MS: i64 = 2_000;
pub const DEFAULT_BATCH_SESSIONS: i64 = 25;
pub const DEFAULT_BACKSTOP_INTERVAL_SECS: i64 = 60;
pub const DEFAULT_BACKSTOP_LOOKBACK_HOURS: i64 = 48;
pub const DEFAULT_SESSION_SETTLE_MINUTES: i64 = 30;

struct IntSetting {
    key: &'static str,
    label: &'static str,
    description: &'static str,
    default: i64,
    min: i64,
    max: i64,
}

fn int_entry(spec: &IntSetting) -> SettingMeta {
    SettingMeta {
        key: spec.key,
        group: "Dossier",
        kind: SettingKind::Int,
        scope: SettingScope::Global,
        applies: SettingApplies::NextCycle,
        default: serde_json::Value::from(spec.default),
        label: spec.label,
        description: spec.description,
        min_i64: Some(spec.min),
        max_i64: Some(spec.max),
        min_f64: None,
        max_f64: None,
        max_len: None,
        enum_variants: None,
    }
}

pub fn entries() -> Vec<SettingMeta> {
    [
        IntSetting {
            key: FLUSH_INTERVAL_MS_KEY,
            label: "Dossier flush interval (ms)",
            description: "How long the dossier builder waits after an operator turn closes \
                before it builds that session's turn facts, so a burst of closes is built in \
                one batch. Read at every backstop tick.",
            default: DEFAULT_FLUSH_INTERVAL_MS,
            min: 250,
            max: 60_000,
        },
        IntSetting {
            key: BATCH_SESSIONS_KEY,
            label: "Dossier sessions per batch",
            description: "The most sessions the dossier builder builds in one batch, for both \
                the close-driven flush and the backstop sweep.",
            default: DEFAULT_BATCH_SESSIONS,
            min: 1,
            max: 500,
        },
        IntSetting {
            key: BACKSTOP_INTERVAL_SECS_KEY,
            label: "Dossier backstop interval (s)",
            description: "How often the dossier builder looks for turns that closed without an \
                event (idle reaps, aborts, a lagged event bus), for turn facts that are \
                pending, stale or built by an older builder, and builds those sessions.",
            default: DEFAULT_BACKSTOP_INTERVAL_SECS,
            min: 10,
            max: 3_600,
        },
        IntSetting {
            key: BACKSTOP_LOOKBACK_HOURS_KEY,
            label: "Dossier backstop lookback (h)",
            description: "How far back, by turn creation time, the backstop sweep looks for \
                turns to build. Older history is covered by 'mike dossier-backfill'.",
            default: DEFAULT_BACKSTOP_LOOKBACK_HOURS,
            min: 1,
            max: 720,
        },
        IntSetting {
            key: SESSION_SETTLE_MINUTES_KEY,
            label: "Dossier session settle time (min)",
            description: "How long a session must be quiet before a turn with no next genuine \
                operator message is recorded as 'none_session_settled' instead of 'pending'. \
                Matches the idle-reap threshold so 'no reply and settled' means the same \
                thing in both places.",
            default: DEFAULT_SESSION_SETTLE_MINUTES,
            min: 5,
            max: 1_440,
        },
    ]
    .iter()
    .map(int_entry)
    .chain(std::iter::once(entries_enabled_entry()))
    .collect()
}

fn entries_enabled_entry() -> SettingMeta {
    SettingMeta {
        key: ENTRIES_ENABLED_KEY,
        group: "Dossier",
        kind: SettingKind::Bool,
        scope: SettingScope::Global,
        applies: SettingApplies::NextCycle,
        default: serde_json::Value::Bool(true),
        label: "Record directives and promises",
        description: "Derive the operator's standing directives (commit, push, deploy, ask \
            first and so on) and the assistant's promises and waits from captured prompts and \
            the work registry. Local only: no LLM call, nothing sent off the machine.",
        min_i64: None,
        max_i64: None,
        min_f64: None,
        max_f64: None,
        max_len: None,
        enum_variants: None,
    }
}
