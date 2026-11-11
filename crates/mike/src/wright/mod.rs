use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

pub mod watch;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Source {
    Logs,
    CaptureDb,
    McpTools,
    Page,
    Hooks,
    Workers,
    Sessions,
}

impl Source {
    pub(crate) const ALL: [Source; 7] = [
        Source::Logs,
        Source::CaptureDb,
        Source::McpTools,
        Source::Page,
        Source::Hooks,
        Source::Workers,
        Source::Sessions,
    ];

    pub(crate) fn code(self) -> &'static str {
        match self {
            Source::Logs => "logs",
            Source::CaptureDb => "capture_db",
            Source::McpTools => "mcp_tools",
            Source::Page => "page",
            Source::Hooks => "hooks",
            Source::Workers => "workers",
            Source::Sessions => "sessions",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Cadence {
    Event,
    Regular,
    Daily,
}

impl Cadence {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Cadence::Event => "event",
            Cadence::Regular => "regular",
            Cadence::Daily => "daily",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct Finding {
    pub(crate) scope: String,
    pub(crate) resource: String,
    pub(crate) evidence: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CheckOutcome {
    Complete(Vec<Finding>),
    Unknown(String),
}

impl CheckOutcome {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            CheckOutcome::Complete(findings) if findings.is_empty() => "pass",
            CheckOutcome::Complete(_) => "fail",
            CheckOutcome::Unknown(_) => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CheckRun<'a> {
    pub(crate) check: &'a str,
    pub(crate) source: Source,
    pub(crate) cadence: Cadence,
    pub(crate) started_at: DateTime<Utc>,
    pub(crate) outcome: &'a CheckOutcome,
    pub(crate) machine: Option<Uuid>,
}
