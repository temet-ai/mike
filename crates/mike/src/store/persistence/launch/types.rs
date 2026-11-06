use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::StderrCapture;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct LaunchRow {
    pub id: i64,
    pub session_uuid: Option<Uuid>,
    pub backend: String,
    #[serde(default)]
    pub backend_reason: Option<String>,
    pub folder: String,
    pub repo_root: Option<String>,
    pub worktree: bool,
    pub workspace_ref: Option<String>,
    pub tab_ref: Option<String>,
    pub surface_ref: Option<String>,
    pub pid: Option<i32>,
    pub run_id: Option<String>,
    pub seat: Option<String>,
    pub template_id: Option<i64>,
    pub model: Option<String>,
    pub agent: Option<String>,
    pub claude_args: serde_json::Value,
    pub prompt_present: bool,
    pub brief_mode: Option<String>,
    pub brief_path: Option<String>,
    pub brief_text: Option<String>,
    pub brief_note_uuid: Option<Uuid>,
    /// When the session took in its brief mail: Claude Code's receipt of the
    /// Remote Control push, or the session's own ack. The signal that a
    /// spawned seat has its brief and has started. NULL until then, and for
    /// a brief that was not mailed.
    pub brief_read_at: Option<DateTime<Utc>>,
    pub requested_placement: Option<String>,
    pub actual_placement: Option<String>,
    pub split_direction: Option<String>,
    pub anchor_ref: Option<String>,
    pub remote_control: bool,
    pub proxy_env: bool,
    pub channel_check: Option<String>,
    pub channel_checked_at: Option<DateTime<Utc>>,
    pub opened: bool,
    pub delivered: Option<bool>,
    pub confirmed: bool,
    pub result: String,
    pub reason: Option<String>,
    pub stderr_text: Option<String>,
    pub stderr_truncated: bool,
    pub created_at: DateTime<Utc>,
    pub opened_at: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
    pub close_reason: Option<String>,
    pub close_process_exited: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewLaunch {
    pub session_uuid: Option<Uuid>,
    pub backend: String,
    pub folder: String,
    pub repo_root: Option<String>,
    pub worktree: bool,
    pub run_id: Option<String>,
    pub seat: Option<String>,
    pub template_id: Option<i64>,
    pub model: Option<String>,
    pub agent: Option<String>,
    pub claude_args: Vec<String>,
    pub prompt_present: bool,
    pub brief_mode: Option<String>,
    pub brief_path: Option<String>,
    pub brief_text: Option<String>,
    pub brief_note_uuid: Option<Uuid>,
    pub requested_placement: Option<String>,
    pub split_direction: Option<String>,
    pub anchor_ref: Option<String>,
    pub remote_control: bool,
    pub proxy_env: bool,
    pub identity: Option<serde_json::Value>,
    pub backend_reason: Option<String>,
}

impl NewLaunch {
    pub fn new(backend: impl Into<String>, folder: impl Into<String>) -> Self {
        Self {
            session_uuid: None,
            backend: backend.into(),
            folder: folder.into(),
            repo_root: None,
            worktree: false,
            run_id: None,
            seat: None,
            template_id: None,
            model: None,
            agent: None,
            claude_args: Vec::new(),
            prompt_present: false,
            brief_mode: None,
            brief_path: None,
            brief_text: None,
            brief_note_uuid: None,
            requested_placement: None,
            split_direction: None,
            anchor_ref: None,
            remote_control: true,
            proxy_env: true,
            identity: None,
            backend_reason: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchSurfaceRefs {
    pub workspace_ref: Option<String>,
    pub tab_ref: Option<String>,
    pub surface_ref: Option<String>,
    pub pid: Option<i32>,
    pub actual_placement: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchOutcome {
    pub result: String,
    pub reason: Option<String>,
    pub opened: bool,
    pub delivered: Option<bool>,
    pub confirmed: bool,
    pub channel_check: Option<String>,
    pub stderr: Option<StderrCapture>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkOutcome {
    Linked,
    AlreadyLinked,
    NoChannel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchListFilter {
    pub run_id: Option<String>,
    pub open_only: bool,
    pub backend: Option<String>,
    pub limit: i64,
}

impl LaunchListFilter {
    pub const DEFAULT_LIMIT: i64 = 200;
}

impl Default for LaunchListFilter {
    fn default() -> Self {
        Self {
            run_id: None,
            open_only: false,
            backend: None,
            limit: Self::DEFAULT_LIMIT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRow {
    pub run_id: String,
    pub project_path: Option<String>,
    pub backend: String,
    pub workspace_ref: Option<String>,
    pub tab_ref: Option<String>,
    pub created_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewRun {
    pub run_id: String,
    pub project_path: Option<String>,
    pub backend: String,
    pub workspace_ref: Option<String>,
    pub tab_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendValidationRow {
    pub id: i64,
    pub backend: String,
    pub passed: bool,
    pub version: Option<String>,
    pub protocol: Option<String>,
    pub checks: serde_json::Value,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBackendValidation {
    pub backend: String,
    pub passed: bool,
    pub version: Option<String>,
    pub protocol: Option<String>,
    pub checks: serde_json::Value,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TemplateRow {
    pub id: i64,
    pub name: String,
    pub session_uuid: Uuid,
    pub folder: String,
    pub backend: String,
    pub model: Option<String>,
    pub agent: Option<String>,
    pub claude_args: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub superseded_by: Option<i64>,
    pub closed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTemplate {
    pub name: String,
    pub session_uuid: Uuid,
    pub folder: String,
    pub backend: String,
    pub model: Option<String>,
    pub agent: Option<String>,
    pub claude_args: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InsertedTemplate {
    pub id: i64,
    pub replaced_session: Option<Uuid>,
}
