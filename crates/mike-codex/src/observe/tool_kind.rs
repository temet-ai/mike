use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Shell,
    Poll,
    CodeMode,
    SubagentControl,
    Mcp,
    Hosted,
    Other,
}

impl ToolKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shell => "shell",
            Self::Poll => "poll",
            Self::CodeMode => "code_mode",
            Self::SubagentControl => "subagent_control",
            Self::Mcp => "mcp",
            Self::Hosted => "hosted",
            Self::Other => "other",
        }
    }
}

const HOSTED_TOOLS: [&str; 4] = [
    "web_search",
    "image_generation",
    "file_search",
    "code_interpreter",
];
const COLLABORATION: &str = "collaboration";

pub fn from_hook_name(name: &str) -> ToolKind {
    if name == "Bash" {
        return ToolKind::Shell;
    }
    if name.starts_with(COLLABORATION) {
        return ToolKind::SubagentControl;
    }
    if name.starts_with("mcp__") {
        return ToolKind::Mcp;
    }
    if HOSTED_TOOLS.contains(&name) {
        return ToolKind::Hosted;
    }
    ToolKind::Other
}

pub fn from_telemetry(namespace: Option<&str>, name: &str, mcp_server: Option<&str>) -> ToolKind {
    if mcp_server.is_some_and(|server| !server.is_empty()) {
        return ToolKind::Mcp;
    }
    if namespace == Some(COLLABORATION) {
        return ToolKind::SubagentControl;
    }
    match name {
        "exec_command" | "shell" | "local_shell" => ToolKind::Shell,
        "write_stdin" => ToolKind::Poll,
        "exec" => ToolKind::CodeMode,
        hosted if HOSTED_TOOLS.contains(&hosted) => ToolKind::Hosted,
        _ => ToolKind::Other,
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct CoverageGap {
    pub path: &'static str,
    pub what_is_missing: &'static str,
}

pub const COVERAGE_GAPS: [CoverageGap; 7] = [
    CoverageGap {
        path: "hosted tools (web_search, image_generation, file_search, code_interpreter)",
        what_is_missing: "run on OpenAI's side; codex 0.167.1 sends no PreToolUse or PostToolUse hook for them, so mike sees at most a telemetry record and never their input or output",
    },
    CoverageGap {
        path: "write_stdin polls of a long-running command",
        what_is_missing: "no hook fires for a poll; telemetry can identify a process session, but common item metadata does not infer a parent command or task completion",
    },
    CoverageGap {
        path: "code-mode nested tools",
        what_is_missing: "the nested call carries no id of the outer `exec` call; it is linked to it only by thread and time, and hooks report the nested call under its own name (Bash)",
    },
    CoverageGap {
        path: "telemetry tool output",
        what_is_missing: "codex.tool_result carries an output snippet, not the whole output; it is never stored as captured output",
    },
    CoverageGap {
        path: "model request attempts",
        what_is_missing: "hooks and telemetry share no model-call id with wire observations; they update activity but are never added to completed model-call counts or usage",
    },
    CoverageGap {
        path: "async hook arrival time",
        what_is_missing: "hook payloads carry no timestamp; with async handlers the time is when mike received the delivery, not when Codex fired it",
    },
    CoverageGap {
        path: "PermissionRequest, PreCompact, PostCompact and Interrupt hooks",
        what_is_missing: "common metadata is accepted, but these events were not exercised in the 0.167.1 recordings; payload bodies are not collected or interpreted",
    },
];
