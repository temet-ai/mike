use std::sync::LazyLock;

use regex::Regex;

use super::PromptKind;

fn compile(source: &str) -> Regex {
    match Regex::new(source) {
        Ok(re) => re,
        Err(e) => panic!("invalid static template `{source}`: {e}"),
    }
}

const TEMPLATE_SOURCES: [(&str, PromptKind); 24] = [
    (r"^\s*Another Claude session sent", PromptKind::PeerMessage),
    (
        r"^\s*<(teammate-message|agent-message)",
        PromptKind::PeerMessage,
    ),
    (
        r"^\s*\[\d+ prior /loop wakeups? found",
        PromptKind::LoopWakeup,
    ),
    (r#"^\s*<channel source="wake""#, PromptKind::ChannelWake),
    (r"^\s*# Autonomous loop check", PromptKind::LoopWakeup),
    (
        r"(?i)^\s*(acceptance|coordinator|observer) seat for fleet [^\n]+ \([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\): (read inbox with get_session_messages|heartbeat)",
        PromptKind::LoopWakeup,
    ),
    (
        r"(?i)(ScheduleWakeup|<<autonomous-loop|re-arm)",
        PromptKind::LoopWakeup,
    ),
    (
        r"(?i)^\s*(fleet|coordinator|coordination|observer|observation|ob1|w-\w+|w\d\w*)\b[^\n]{0,80}\b(cycle|sweep)\b",
        PromptKind::LoopWakeup,
    ),
    (
        r"(?i)^\s*check\s+(your\s+)?(mailbox|inbox)",
        PromptKind::LoopWakeup,
    ),
    (
        r"(?i)^\s*(continue|resume)\s+(as\s+)?(w\d\S*|the\s+fleet\S*|fleet\S*|rt-\S+|#\d+)",
        PromptKind::LoopWakeup,
    ),
    (
        r"(?i)^\s*you\s+are\s+(the\s+)?(w\d\w*|ob\d|observer)\b",
        PromptKind::LoopWakeup,
    ),
    (
        r"(?i)^\s*[a-z]{1,4}\d\w*\s+(fleet\s+)?seat\b",
        PromptKind::LoopWakeup,
    ),
    (r"(?i)^\s*rt-\S+\s+seat", PromptKind::LoopWakeup),
    (r"^\s*<task-notification", PromptKind::TaskNotification),
    (r"^\s*\[SYSTEM NOTIFICATION", PromptKind::BackgroundEvent),
    (r"^\s*Tool loaded\.\s*\z", PromptKind::ToolLoaded),
    (r"^\s*Stop hook feedback:", PromptKind::HookFeedback),
    (
        r"^\s*This session is being continued from a previous",
        PromptKind::Compaction,
    ),
    (
        r"^\s*(Summarize this portion of a Claude Code session|Analyze this Claude Code session and extract)",
        PromptKind::AnalysisProbe,
    ),
    (
        r"^\s*Describe your most recent action in 3-5 words",
        PromptKind::ActivityLabel,
    ),
    (r"^\s*\[SUGGESTION MODE:", PromptKind::AnalysisProbe),
    (
        r"has the following stopping condition been satisfied",
        PromptKind::AnalysisProbe,
    ),
    (r"^\s*<transcript>", PromptKind::AnalysisProbe),
    (
        r"The user stepped away and is coming back",
        PromptKind::SessionRecap,
    ),
];

static TEMPLATES: LazyLock<Vec<(Regex, PromptKind)>> = LazyLock::new(|| {
    TEMPLATE_SOURCES
        .iter()
        .map(|(source, kind)| (compile(source), *kind))
        .collect()
});

static TASK_NOTIFICATION_REWRITE: LazyLock<Regex> =
    LazyLock::new(|| compile(r"^\s*Task notification: "));

pub fn match_template(text: &str) -> Option<PromptKind> {
    TEMPLATES
        .iter()
        .find(|(regex, _)| regex.is_match(text))
        .map(|(_, kind)| *kind)
}

pub fn is_task_notification_rewrite(text: &str) -> bool {
    TASK_NOTIFICATION_REWRITE.is_match(text)
}
