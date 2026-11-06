use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Edit,
    ShellChange,
    Commit,
    Push,
    Check,
    Verdict,
    Note,
    Mail,
    Brief,
    TaskState,
}

impl ChangeKind {
    pub const ALL: [ChangeKind; 10] = [
        ChangeKind::Edit,
        ChangeKind::ShellChange,
        ChangeKind::Commit,
        ChangeKind::Push,
        ChangeKind::Check,
        ChangeKind::Verdict,
        ChangeKind::Note,
        ChangeKind::Mail,
        ChangeKind::Brief,
        ChangeKind::TaskState,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ChangeKind::Edit => "edit",
            ChangeKind::ShellChange => "shell_change",
            ChangeKind::Commit => "commit",
            ChangeKind::Push => "push",
            ChangeKind::Check => "check",
            ChangeKind::Verdict => "verdict",
            ChangeKind::Note => "note",
            ChangeKind::Mail => "mail",
            ChangeKind::Brief => "brief",
            ChangeKind::TaskState => "task_state",
        }
    }

    pub fn parse(text: &str) -> Option<ChangeKind> {
        ChangeKind::ALL.into_iter().find(|k| k.as_str() == text)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewChangeEvent {
    pub project_id: i64,
    pub checkout: String,
    pub at: DateTime<Utc>,
    pub kind: ChangeKind,
    pub session_id: Option<i64>,
    pub run_id: Option<String>,
    pub task: Option<String>,
    pub paths: Vec<String>,
    pub subject: String,
    pub detail: serde_json::Value,
    pub before_sha: Option<String>,
    pub after_sha: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChangeEvent {
    pub id: i64,
    pub project_id: i64,
    pub checkout: String,
    pub at: DateTime<Utc>,
    pub kind: ChangeKind,
    pub session_id: Option<i64>,
    pub run_id: Option<String>,
    pub task: Option<String>,
    pub paths: Vec<String>,
    pub subject: String,
    pub detail: serde_json::Value,
    pub before_sha: Option<String>,
    pub after_sha: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChangeQuery {
    pub checkout: String,
    pub after_id: i64,
    pub path_prefixes: Vec<String>,
    pub exclude_session: Option<i64>,
    pub kinds: Vec<ChangeKind>,
    pub limit: i64,
}

impl ChangeQuery {
    pub fn everything_after(checkout: impl Into<String>, after_id: i64, limit: i64) -> Self {
        Self {
            checkout: checkout.into(),
            after_id,
            path_prefixes: Vec::new(),
            exclude_session: None,
            kinds: Vec::new(),
            limit,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SeenPath {
    pub path: String,
    pub last_seen: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolCallNaming {
    pub session_id: i64,
    pub tool_use_id: i64,
    pub tool_name: String,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellRun {
    pub session_id: i64,
    pub started: DateTime<Utc>,
    pub finished: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpenChange {
    pub path: String,
    pub session_id: Option<i64>,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkingCheckout {
    pub checkout: String,
    pub project_id: i64,
    pub newest_request_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionIdentity {
    pub id: i64,
    pub uuid: uuid::Uuid,
    pub name: Option<String>,
    pub cwd: Option<String>,
    pub project_id: i64,
}
