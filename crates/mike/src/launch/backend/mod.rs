use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::error::LaunchError;

#[cfg(any(test, feature = "test-seams"))]
pub mod fake;
mod none;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendId {
    Herdr,
    #[serde(rename = "herdr-core")]
    HerdrCore,
    Tmux,
    Terminal,
    Ghostty,
    None,
}

impl BackendId {
    pub const ALL: [BackendId; 6] = [
        BackendId::Herdr,
        BackendId::HerdrCore,
        BackendId::Tmux,
        BackendId::Terminal,
        BackendId::Ghostty,
        BackendId::None,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            BackendId::Herdr => "herdr",
            BackendId::HerdrCore => "herdr-core",
            BackendId::Tmux => "tmux",
            BackendId::Terminal => "terminal",
            BackendId::Ghostty => "ghostty",
            BackendId::None => "none",
        }
    }
}

impl fmt::Display for BackendId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for BackendId {
    type Err = LaunchError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        BackendId::ALL
            .into_iter()
            .find(|id| id.as_str() == s)
            .ok_or_else(|| {
                LaunchError::Usage(format!(
                    "unknown terminal backend `{s}`; one of herdr, herdr-core, tmux, terminal, ghostty, none"
                ))
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Capabilities {
    pub placement_workspace: bool,
    pub placement_tab: bool,
    pub placement_split: bool,
    pub agent_state: bool,
    pub prompt_in_argv: bool,
    pub close_by_session: bool,
    pub events: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    pub name: String,
    pub passed: bool,
    pub detail: String,
    pub remedy: Option<String>,
}

impl Check {
    pub fn pass(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: true,
            detail: detail.into(),
            remedy: None,
        }
    }

    pub fn fail(
        name: impl Into<String>,
        detail: impl Into<String>,
        remedy: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            passed: false,
            detail: detail.into(),
            remedy: Some(remedy.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbeReport {
    pub backend: BackendId,
    pub available: bool,
    pub version: Option<String>,
    pub protocol: Option<String>,
    pub checks: Vec<Check>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationReport {
    pub backend: BackendId,
    pub passed: bool,
    pub checks: Vec<Check>,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
}

impl ValidationReport {
    pub fn from_checks(backend: BackendId, checks: Vec<Check>, started_at: DateTime<Utc>) -> Self {
        Self {
            backend,
            passed: checks.iter().all(|c| c.passed),
            checks,
            started_at,
            finished_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitDirection {
    Left,
    Right,
    Up,
    Down,
}

impl SplitDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            SplitDirection::Left => "left",
            SplitDirection::Right => "right",
            SplitDirection::Up => "up",
            SplitDirection::Down => "down",
        }
    }
}

impl FromStr for SplitDirection {
    type Err = LaunchError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "l" | "left" => Ok(SplitDirection::Left),
            "r" | "right" => Ok(SplitDirection::Right),
            "u" | "up" => Ok(SplitDirection::Up),
            "d" | "down" => Ok(SplitDirection::Down),
            other => Err(LaunchError::Usage(format!(
                "--split takes one of l|r|u|d (left|right|up|down), not `{other}`"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    Window,
    Tab,
    Split(SplitDirection),
    Panel,
}

impl Placement {
    pub fn as_str(self) -> String {
        match self {
            Placement::Window => "window".to_string(),
            Placement::Tab => "tab".to_string(),
            Placement::Split(dir) => format!("split:{}", dir.as_str()),
            Placement::Panel => "panel".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSeat {
    pub run_id: String,
    pub workspace_ref: Option<String>,
    pub seat: Option<String>,
    pub role: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub body: String,
    pub urgent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneRequest {
    pub folder: PathBuf,
    pub run: RunSeat,
    pub command: String,
}

#[derive(Debug, Clone)]
pub struct OpenRequest {
    pub folder: PathBuf,
    pub placement: Placement,
    pub env: BTreeMap<String, String>,
    pub title: String,
    pub run_tab: Option<String>,
    pub run: Option<RunSeat>,
    pub program: String,
    pub args: Vec<String>,
    pub name: String,
    pub prompt: Option<String>,
    pub prompt_wait_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Surface {
    pub backend: BackendId,
    pub workspace_ref: Option<String>,
    pub tab_ref: Option<String>,
    pub surface_ref: String,
    pub pid: Option<u32>,
    pub session_uuid: Option<Uuid>,
    pub agent_state: Option<String>,
    pub cwd: Option<String>,
    pub actual_placement: String,
    pub reused: bool,
    pub delivered: Option<bool>,
    pub confirmed: bool,
    pub prompt_detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloseOutcome {
    Closed { process_exited: bool },
    NotFound,
    Mismatch { detail: String },
}

#[async_trait]
pub trait TerminalBackend: Send + Sync {
    fn id(&self) -> BackendId;
    fn capabilities(&self) -> Capabilities;
    async fn probe(&self) -> ProbeReport;
    async fn validate(&self) -> ValidationReport;
    async fn open(&self, req: &OpenRequest) -> Result<Surface, LaunchError>;
    async fn report_pane(&self, _surface_ref: &str, _values: &[(String, String)]) {}
    async fn report_workspace(&self, _workspace_ref: &str, _values: &[(String, String)]) {}
    async fn notify(&self, _notice: &Notice) {}
    async fn open_pane(&self, req: &PaneRequest) -> Result<Surface, LaunchError> {
        Err(LaunchError::Unsupported(format!(
            "the {} backend cannot open a plain pane for `{}`",
            self.id(),
            req.command
        )))
    }
    async fn close_workspace(&self, workspace_ref: &str) -> Result<(), LaunchError> {
        Err(LaunchError::Unsupported(format!(
            "the {} backend cannot close the workspace `{workspace_ref}`",
            self.id()
        )))
    }
    async fn find_by_session(&self, session: Uuid) -> Result<Option<Surface>, LaunchError>;
    async fn close(&self, surface: &Surface, wait: Duration) -> Result<CloseOutcome, LaunchError>;
    async fn list(&self) -> Result<Vec<Surface>, LaunchError>;

    /// Submit a prompt to the agent `name` in an already open surface, and
    /// report whether it was seen to become a turn. Used for the note-brief
    /// fallback, when the brief could not be pushed over Remote Control.
    async fn send_prompt(&self, name: &str, text: &str, wait_ms: u64) -> Result<bool, LaunchError> {
        let _ = (name, text, wait_ms);
        Err(LaunchError::Unsupported(format!(
            "{} cannot submit a prompt to an open surface",
            self.id().as_str()
        )))
    }
}

pub fn backend_for(id: BackendId) -> Option<Box<dyn TerminalBackend>> {
    match id {
        BackendId::Herdr => Some(Box::new(super::herdr::HerdrBackend::new())),
        BackendId::HerdrCore => Some(Box::new(super::herdr_core::HerdrCoreBackend::new())),
        BackendId::Tmux => Some(Box::new(super::tmux::TmuxBackend::new())),
        BackendId::Terminal => Some(Box::new(super::terminal::XdgTerminalBackend::new())),
        BackendId::None => Some(Box::new(none::NoneBackend)),
        BackendId::Ghostty => None,
    }
}

#[cfg(test)]
mod names_tests;

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
mod tests {
    use super::*;

    #[test]
    fn backend_id_round_trips_through_str_and_serde() {
        for id in BackendId::ALL {
            assert_eq!(id.as_str().parse::<BackendId>().ok(), Some(id));
            let json = serde_json::to_string(&id).unwrap();
            assert_eq!(json, format!("\"{id}\""));
            assert_eq!(serde_json::from_str::<BackendId>(&json).unwrap(), id);
        }
    }

    #[test]
    fn backend_id_unknown_name_is_usage_error() {
        let err = "screen".parse::<BackendId>().unwrap_err();
        assert_eq!(err.exit_code(), 1);
        assert!(err.to_string().contains("screen"));
    }

    #[test]
    fn split_direction_accepts_short_and_long_forms() {
        assert_eq!("l".parse::<SplitDirection>().unwrap(), SplitDirection::Left);
        assert_eq!(
            "down".parse::<SplitDirection>().unwrap(),
            SplitDirection::Down
        );
        assert!("sideways".parse::<SplitDirection>().is_err());
    }

    #[test]
    fn backend_for_herdr_tmux_terminal_and_none_exist_ghostty_absent() {
        assert!(backend_for(BackendId::Herdr).is_some());
        assert!(backend_for(BackendId::Tmux).is_some());
        assert!(backend_for(BackendId::Terminal).is_some());
        assert!(backend_for(BackendId::None).is_some());
        assert!(backend_for(BackendId::Ghostty).is_none());
    }

    #[test]
    fn validation_report_passed_only_when_every_check_passed() {
        let now = Utc::now();
        let ok = ValidationReport::from_checks(BackendId::None, vec![Check::pass("a", "")], now);
        assert!(ok.passed);
        let bad = ValidationReport::from_checks(
            BackendId::None,
            vec![Check::pass("a", ""), Check::fail("b", "d", "r")],
            now,
        );
        assert!(!bad.passed);
        assert!(bad.finished_at >= bad.started_at);
    }
}
