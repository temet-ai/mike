use crate::settings::{SettingApplies, SettingKind, SettingMeta, SettingScope};

pub const DEFAULT_MIN_RESULT_BYTES: usize = 1500;
pub const DEFAULT_HEAD_LINES: usize = 20;
pub const DEFAULT_TAIL_LINES: usize = 20;
pub const DEFAULT_SIGNAL_CONTEXT_LINES: usize = 3;
pub const DEFAULT_SIGNAL_LINE_CAP: usize = 40;
pub const DEFAULT_GRACE_TURNS: usize = 3;

const GROUP: &str = "Shell-output compression";

struct IntEntry {
    key: &'static str,
    default: usize,
    label: &'static str,
    description: &'static str,
    range: (i64, i64),
}

fn int_entry(entry: IntEntry) -> SettingMeta {
    SettingMeta {
        key: entry.key,
        group: GROUP,
        kind: SettingKind::Int,
        scope: SettingScope::Session,
        applies: SettingApplies::NewSessions,
        default: serde_json::Value::Number(serde_json::Number::from(entry.default as u64)),
        label: entry.label,
        description: entry.description,
        min_i64: Some(entry.range.0),
        max_i64: Some(entry.range.1),
        min_f64: None,
        max_f64: None,
        max_len: None,
        enum_variants: None,
    }
}

fn mode_entry() -> SettingMeta {
    SettingMeta {
        key: "shell_output_compression.mode",
        group: GROUP,
        kind: SettingKind::Enum,
        scope: SettingScope::Session,
        applies: SettingApplies::NewSessions,
        default: serde_json::Value::String("shadow".to_owned()),
        label: "Mode",
        description: "Forward sends every new session shortened requests, whatever the share. Shadow sends shortened requests only to the share of new sessions set below and records what would be shortened for the rest; shadow with a share of 0 never forwards. Sessions already running keep the policy they started with.",
        min_i64: None,
        max_i64: None,
        min_f64: None,
        max_f64: None,
        max_len: None,
        enum_variants: Some(&["shadow", "forward"]),
    }
}

pub fn entries() -> Vec<SettingMeta> {
    let ints = [
        IntEntry {
            key: "shell_output_compression.min_bytes",
            default: DEFAULT_MIN_RESULT_BYTES,
            label: "Minimum bytes before reduction",
            description: "Bash output shorter than this many bytes is never shortened.",
            range: (1, 10_485_760),
        },
        IntEntry {
            key: "shell_output_compression.head_lines",
            default: DEFAULT_HEAD_LINES,
            label: "Head lines kept",
            description: "Lines kept from the start of shortened Bash output.",
            range: (0, 5000),
        },
        IntEntry {
            key: "shell_output_compression.tail_lines",
            default: DEFAULT_TAIL_LINES,
            label: "Tail lines kept",
            description: "Lines kept from the end of shortened Bash output.",
            range: (0, 5000),
        },
        IntEntry {
            key: "shell_output_compression.signal_context_lines",
            default: DEFAULT_SIGNAL_CONTEXT_LINES,
            label: "Lines kept after each error or summary line",
            description: "Every error, failure, warning or summary line in the middle of the output is kept, with this many lines after it.",
            range: (0, 100),
        },
        IntEntry {
            key: "shell_output_compression.signal_line_cap",
            default: DEFAULT_SIGNAL_LINE_CAP,
            label: "Most error or summary lines kept",
            description: "At most this many error or summary lines are kept from the middle of one output; the rest stay in the omitted range, recoverable with the original.",
            range: (0, 10_000),
        },
        IntEntry {
            key: "shell_output_compression.grace_turns",
            default: DEFAULT_GRACE_TURNS,
            label: "Answers before output is shortened",
            description: "Bash output stays whole until the model has answered this many times after it.",
            range: (0, 1000),
        },
        IntEntry {
            key: "shell_output_compression.forward_share",
            default: 0,
            label: "Share of sessions forwarded",
            description: "In shadow mode, the percent of new sessions, chosen by session id, that are sent shortened Bash output; the rest are the control. Forward mode ignores it. Each request's manifest records its session's arm.",
            range: (0, 100),
        },
    ];
    let mut entries: Vec<SettingMeta> = ints.into_iter().map(int_entry).collect();
    entries.insert(6, mode_entry());
    entries
}
