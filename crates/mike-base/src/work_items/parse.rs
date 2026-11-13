use serde_json::Value;

pub const BASH_LAUNCH_PREFIX: &str = "Command running in background with ID: ";
pub const AGENT_LAUNCH_PREFIX: &str = "Async agent launched successfully.";
pub const NOTIFICATION_OPEN: &str = "<task-notification>";
const NOTIFICATION_CLOSE: &str = "</task-notification>";
const OUTPUT_MARK: &str = "Output is being written to: ";
const AGENT_ID_MARK: &str = "agentId: ";
const AGENT_OUTPUT_MARK: &str = "output_file: ";
const EXIT_CODE_MARK: &str = "(exit code ";
pub const RESUME_MARK: &str = "\"resumedAgentId\":\"";
pub const STOP_MARK: &str = "Successfully stopped task: ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkKind {
    Bash,
    Agent,
}

impl WorkKind {
    pub fn code(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Agent => "agent",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub kind: WorkKind,
    pub task_id: String,
    pub output_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub task_id: String,
    pub tool_use_id: Option<String>,
    pub status: String,
    pub summary: Option<String>,
    pub exit_code: Option<i32>,
}

impl Notification {
    pub fn state(&self) -> &'static str {
        match self.status.as_str() {
            "completed" if self.exit_code.is_some_and(|c| c != 0) => "failed",
            "completed" => "completed",
            "failed" => "failed",
            "killed" | "stopped" => "killed",
            _ => "unknown",
        }
    }
}

pub fn result_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

pub fn parse_launch(tool_name: &str, text: &str) -> Option<Launch> {
    let text = text.trim_start();
    match tool_name {
        "Bash" => parse_bash_launch(text),
        "Agent" | "Task" => parse_agent_launch(text),
        _ => None,
    }
}

fn parse_bash_launch(text: &str) -> Option<Launch> {
    let rest = text.strip_prefix(BASH_LAUNCH_PREFIX)?;
    Some(Launch {
        kind: WorkKind::Bash,
        task_id: take_token(rest)?,
        output_path: after_mark(text, OUTPUT_MARK).map(take_path),
    })
}

fn parse_agent_launch(text: &str) -> Option<Launch> {
    if !text.starts_with(AGENT_LAUNCH_PREFIX) {
        return None;
    }
    Some(Launch {
        kind: WorkKind::Agent,
        task_id: after_mark(text, AGENT_ID_MARK).and_then(take_token)?,
        output_path: after_mark(text, AGENT_OUTPUT_MARK).map(take_path),
    })
}

pub fn parse_notifications(text: &str) -> Vec<Notification> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(NOTIFICATION_OPEN) {
        let body_start = start + NOTIFICATION_OPEN.len();
        let Some(len) = rest[body_start..].find(NOTIFICATION_CLOSE) else {
            break;
        };
        if let Some(note) = parse_one_notification(&rest[body_start..body_start + len]) {
            found.push(note);
        }
        rest = &rest[body_start + len + NOTIFICATION_CLOSE.len()..];
    }
    found
}

pub fn notifications_in_value(value: &Value) -> Vec<Notification> {
    let mut found = Vec::new();
    collect_notifications(value, &mut found);
    found
}

pub fn notifications_in_request_body(body: &[u8]) -> Vec<Notification> {
    let Ok(raw) = std::str::from_utf8(body) else {
        return Vec::new();
    };
    if !raw.contains("task-notification>") {
        return Vec::new();
    }
    let Ok(parsed) = serde_json::from_str::<Value>(raw) else {
        return Vec::new();
    };
    parsed
        .get("messages")
        .and_then(Value::as_array)
        .and_then(|messages| messages.last())
        .map(notifications_in_value)
        .unwrap_or_default()
}

fn collect_notifications(value: &Value, found: &mut Vec<Notification>) {
    match value {
        Value::String(s) if s.contains(NOTIFICATION_OPEN) => found.extend(parse_notifications(s)),
        Value::Array(items) => items.iter().for_each(|v| collect_notifications(v, found)),
        Value::Object(map) if map.get("type").and_then(Value::as_str) == Some("tool_result") => {}
        Value::Object(map) => map.values().for_each(|v| collect_notifications(v, found)),
        _ => {}
    }
}

fn parse_one_notification(body: &str) -> Option<Notification> {
    let summary = tag(body, "summary");
    let exit_code = summary.as_deref().and_then(exit_code_of);
    Some(Notification {
        task_id: tag(body, "task-id")?,
        tool_use_id: tag(body, "tool-use-id"),
        status: tag(body, "status")?,
        summary,
        exit_code,
    })
}

fn tag(body: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let start = body.find(&open)? + open.len();
    let end = body[start..].find(&close)? + start;
    let value = body[start..end].trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn exit_code_of(summary: &str) -> Option<i32> {
    let rest = after_mark(summary, EXIT_CODE_MARK)?;
    let digits: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '-')
        .collect();
    digits.parse().ok()
}

fn after_mark<'a>(text: &'a str, mark: &str) -> Option<&'a str> {
    text.find(mark).map(|i| &text[i + mark.len()..])
}

fn take_token(text: &str) -> Option<String> {
    let token: String = text
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    (!token.is_empty()).then_some(token)
}

fn take_path(text: &str) -> String {
    let end = text.find(char::is_whitespace).unwrap_or(text.len());
    text[..end].trim_end_matches('.').to_string()
}

pub fn parse_stop(tool_name: &str, text: &str) -> Option<Notification> {
    if tool_name != "TaskStop" {
        return None;
    }
    Some(Notification {
        task_id: after_mark(text, STOP_MARK).and_then(take_token)?,
        tool_use_id: None,
        status: "stopped".to_owned(),
        summary: Some("stopped with TaskStop".to_owned()),
        exit_code: None,
    })
}

pub fn parse_resume(tool_name: &str, text: &str) -> Option<String> {
    if tool_name != "SendMessage" {
        return None;
    }
    after_mark(text, RESUME_MARK).and_then(take_token)
}
