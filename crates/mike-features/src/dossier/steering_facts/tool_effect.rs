use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;
use serde_json::Value;

use crate::dossier::build::checks::check_kind;

pub const TOOL_EFFECT_VERSION: i16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Edit,
    Vcs,
    Fs,
    Deps,
    Deploy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationKind {
    Test,
    Build,
    Lint,
    Gate,
    Run,
    Http,
    Browser,
    Render,
    DeployPoll,
}

pub const READ_BACK_KIND: &str = "read_back";

impl ObservationKind {
    pub const ALL: [Self; 9] = [
        Self::Test,
        Self::Build,
        Self::Lint,
        Self::Gate,
        Self::Run,
        Self::Http,
        Self::Browser,
        Self::Render,
        Self::DeployPoll,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::Test => "test",
            Self::Build => "build",
            Self::Lint => "lint",
            Self::Gate => "gate",
            Self::Run => "run",
            Self::Http => "http",
            Self::Browser => "browser",
            Self::Render => "render",
            Self::DeployPoll => "deploy_poll",
        }
    }
}

pub fn recordable_observation_kinds() -> Vec<&'static str> {
    ObservationKind::ALL
        .iter()
        .map(|kind| kind.code())
        .chain(std::iter::once(READ_BACK_KIND))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "effect", content = "kind", rename_all = "snake_case")]
pub enum ToolEffect {
    Mutating(ChangeKind),
    Observing(ObservationKind),
    Neutral,
}

fn compile(pattern: &str) -> Regex {
    match Regex::new(pattern) {
        Ok(re) => re,
        Err(e) => panic!("invalid tool-effect pattern {pattern:?}: {e}"),
    }
}

static VCS: LazyLock<Regex> = LazyLock::new(|| {
    compile(r"\bgit\s+(commit|push|merge|rebase|cherry-pick|revert|reset|apply|am)([\s;&|)]|$)")
});
static FS: LazyLock<Regex> =
    LazyLock::new(|| compile(r"(^|[;&|]\s*|\s)(mv|cp|rm|mkdir|ln|install)\s|\bsed\s+-i\b|\btee\s"));
static REDIRECT: LazyLock<Regex> = LazyLock::new(|| compile(r">>?\s*([^\s&|;>]+)"));
static DEPS: LazyLock<Regex> =
    LazyLock::new(|| compile(r"\b(npm|pnpm|yarn|cargo|pip|uv)\s+(install|add|remove|update)\b"));
static DEPLOY: LazyLock<Regex> = LazyLock::new(|| {
    compile(
        r"\bdeploy|\bkubectl\s+(apply|rollout|set)\b|\bdocker\s+compose\s+up\b|\bmake\s+install\b|\bgh\s+pr\s+(create|merge)\b|\bgh\s+release\b",
    )
});
static DEPLOY_POLL: LazyLock<Regex> = LazyLock::new(|| {
    compile(
        r"\bgh\s+(run\s+(view|watch)|pr\s+checks)\b|\bscripts/task-wait\b|\bkubectl\s+(get|rollout\s+status)\b",
    )
});
static HTTP: LazyLock<Regex> = LazyLock::new(|| compile(r"\b(curl|wget)\b|\bhttp\s"));
static RUN: LazyLock<Regex> = LazyLock::new(|| {
    compile(
        r"\./target/|(^|\s)mike\s|(^|\s)mike-mcp\s|(^|\s)node\s|\bpython3?\s+\S*\.py\b|\bdocker\s+(ps|logs)\b",
    )
});
static RENDER_PATH: LazyLock<Regex> = LazyLock::new(|| compile(r"(?i)\.(png|jpe?g|gif|webp|pdf)$"));

const BROWSER_ACTIONS: [&str; 9] = [
    "navigate",
    "read_page",
    "get_page_text",
    "find",
    "javascript_tool",
    "read_console_messages",
    "read_network_requests",
    "browser_batch",
    "computer",
];

fn outside_quotes(command: &str) -> String {
    let mut out = String::with_capacity(command.len());
    let mut open: Option<char> = None;
    let mut escaped = false;
    for ch in command.chars() {
        match open {
            Some('"') if escaped => escaped = false,
            Some('"') if ch == '\\' => escaped = true,
            Some(quote) if ch == quote => open = None,
            Some(_) => {}
            None if ch == '\'' || ch == '"' => open = Some(ch),
            None => out.push(ch),
        }
    }
    out
}

fn redirects_to_a_file(command: &str) -> bool {
    let bare = outside_quotes(command);
    REDIRECT
        .captures_iter(&bare)
        .filter_map(|c| c.get(1))
        .any(|target| target.as_str() != "/dev/null" && !target.as_str().starts_with('&'))
}

fn bash_change(command: &str) -> Option<ChangeKind> {
    if VCS.is_match(command) {
        return Some(ChangeKind::Vcs);
    }
    if DEPLOY.is_match(command) && !DEPLOY_POLL.is_match(command) {
        return Some(ChangeKind::Deploy);
    }
    if DEPS.is_match(command) {
        return Some(ChangeKind::Deps);
    }
    if FS.is_match(command) || redirects_to_a_file(command) {
        return Some(ChangeKind::Fs);
    }
    None
}

fn check_observation(command: &str) -> Option<ObservationKind> {
    match check_kind(command)? {
        "test" => Some(ObservationKind::Test),
        "lint" | "typecheck" => Some(ObservationKind::Lint),
        "build" | "migrate" => Some(ObservationKind::Build),
        "gate" | "perf" => Some(ObservationKind::Gate),
        _ => None,
    }
}

fn bash_observation(command: &str) -> Option<ObservationKind> {
    if let Some(kind) = check_observation(command) {
        return Some(kind);
    }
    if DEPLOY_POLL.is_match(command) {
        return Some(ObservationKind::DeployPoll);
    }
    if HTTP.is_match(command) {
        return Some(ObservationKind::Http);
    }
    RUN.is_match(command).then_some(ObservationKind::Run)
}

fn classify_bash(input: &Value) -> ToolEffect {
    let command = input
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if let Some(change) = bash_change(command) {
        return ToolEffect::Mutating(change);
    }
    bash_observation(command).map_or(ToolEffect::Neutral, ToolEffect::Observing)
}

fn classify_browser(tool_name: &str, input: &Value) -> ToolEffect {
    let action = tool_name.rsplit("__").next().unwrap_or_default();
    if !BROWSER_ACTIONS.contains(&action) {
        return ToolEffect::Neutral;
    }
    if action == "computer" {
        let screenshot = input.get("action").and_then(Value::as_str) == Some("screenshot");
        return if screenshot {
            ToolEffect::Observing(ObservationKind::Browser)
        } else {
            ToolEffect::Neutral
        };
    }
    ToolEffect::Observing(ObservationKind::Browser)
}

fn classify_read(input: &Value) -> ToolEffect {
    let path = input
        .get("file_path")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if RENDER_PATH.is_match(path) {
        ToolEffect::Observing(ObservationKind::Render)
    } else {
        ToolEffect::Neutral
    }
}

pub fn classify(tool_name: &str, input: &Value) -> ToolEffect {
    match tool_name {
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => ToolEffect::Mutating(ChangeKind::Edit),
        "Bash" => classify_bash(input),
        "WebFetch" => ToolEffect::Observing(ObservationKind::Http),
        "Read" => classify_read(input),
        name if name.starts_with("mcp__claude-in-chrome__")
            || name.starts_with("mcp__playwright__") =>
        {
            classify_browser(name, input)
        }
        _ => ToolEffect::Neutral,
    }
}
