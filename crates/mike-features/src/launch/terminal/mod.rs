pub mod cli;
pub mod close;
pub mod open;
pub mod probe;
pub mod surface;
#[cfg(test)]
pub mod tests;
pub mod validate;

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use uuid::Uuid;

use super::backend::{
    BackendId, Capabilities, CloseOutcome, OpenRequest, ProbeReport, Surface, TerminalBackend,
    ValidationReport,
};
use super::error::LaunchError;
use cli::TerminalCli;

#[derive(Debug, Clone, Copy)]
pub struct TerminalTimeouts {
    pub window: Duration,
    pub window_poll: Duration,
    pub child_pid: Duration,
    pub marker: Duration,
    pub self_close: Duration,
    pub forced_close: Duration,
}

impl Default for TerminalTimeouts {
    fn default() -> Self {
        Self {
            window: Duration::from_secs(5),
            window_poll: Duration::from_millis(200),
            child_pid: Duration::from_secs(5),
            marker: Duration::from_secs(5),
            self_close: Duration::from_secs(5),
            forced_close: Duration::from_secs(5),
        }
    }
}

pub struct XdgTerminalBackend {
    cli: TerminalCli,
    timeouts: TerminalTimeouts,
}

impl Default for XdgTerminalBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl XdgTerminalBackend {
    pub fn new() -> Self {
        Self::with_binaries("xdg-terminal-exec", "hyprctl")
    }

    pub fn with_binaries(xdg_bin: impl Into<PathBuf>, hypr_bin: impl Into<PathBuf>) -> Self {
        Self {
            cli: TerminalCli::new(xdg_bin, hypr_bin),
            timeouts: TerminalTimeouts::default(),
        }
    }

    #[cfg(any(test, feature = "test-seams"))]
    fn with_binaries_and_timeouts(
        xdg_bin: impl Into<PathBuf>,
        hypr_bin: impl Into<PathBuf>,
        timeouts: TerminalTimeouts,
    ) -> Self {
        Self {
            cli: TerminalCli::new(xdg_bin, hypr_bin),
            timeouts,
        }
    }
}

#[async_trait]
impl TerminalBackend for XdgTerminalBackend {
    fn id(&self) -> BackendId {
        BackendId::Terminal
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            placement_workspace: false,
            placement_tab: false,
            placement_split: false,
            agent_state: false,
            prompt_in_argv: true,
            close_by_session: true,
            events: false,
        }
    }

    async fn probe(&self) -> ProbeReport {
        probe::probe(&self.cli).await
    }

    async fn validate(&self) -> ValidationReport {
        validate::validate(&self.cli, self.timeouts).await
    }

    async fn open(&self, req: &OpenRequest) -> Result<Surface, LaunchError> {
        open::open(&self.cli, req, self.timeouts).await
    }

    async fn find_by_session(&self, session: Uuid) -> Result<Option<Surface>, LaunchError> {
        surface::find_by_session(&self.cli, session, cli::DEFAULT_TIMEOUT).await
    }

    async fn close(&self, surface: &Surface, wait: Duration) -> Result<CloseOutcome, LaunchError> {
        close::close(&self.cli, surface, wait).await
    }

    async fn list(&self) -> Result<Vec<Surface>, LaunchError> {
        surface::list(&self.cli, cli::DEFAULT_TIMEOUT).await
    }
}
