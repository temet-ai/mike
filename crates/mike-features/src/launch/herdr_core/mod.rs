pub mod cli;
pub mod close;
pub mod open;
pub mod place;
pub mod probe;
pub mod prompt;
pub mod terminal;
#[cfg(test)]
pub mod tests;

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use uuid::Uuid;

use super::backend::{
    BackendId, Capabilities, CloseOutcome, OpenRequest, PaneRequest, ProbeReport, Surface,
    TerminalBackend, ValidationReport,
};
use super::error::LaunchError;
use cli::HcCli;

pub struct HerdrCoreBackend {
    cli: HcCli,
    caller: Option<String>,
}

impl Default for HerdrCoreBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl HerdrCoreBackend {
    pub fn new() -> Self {
        Self::with_binaries("hc", "herdr-app")
    }

    pub fn with_binaries(hc: impl Into<PathBuf>, app: impl Into<PathBuf>) -> Self {
        Self {
            cli: HcCli::new(hc, app),
            caller: None,
        }
    }

    pub fn with_caller(mut self, terminal: Option<String>) -> Self {
        self.caller = terminal;
        self
    }
}

#[async_trait]
impl TerminalBackend for HerdrCoreBackend {
    fn id(&self) -> BackendId {
        BackendId::HerdrCore
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            placement_workspace: true,
            placement_tab: true,
            placement_split: false,
            agent_state: true,
            prompt_in_argv: false,
            close_by_session: true,
            events: false,
        }
    }

    async fn probe(&self) -> ProbeReport {
        probe::probe(&self.cli).await
    }

    async fn validate(&self) -> ValidationReport {
        probe::validate(&self.cli).await
    }

    async fn open(&self, req: &OpenRequest) -> Result<Surface, LaunchError> {
        open::open(&self.cli, self.caller.as_deref(), req).await
    }

    async fn open_pane(&self, req: &PaneRequest) -> Result<Surface, LaunchError> {
        open::open_pane(&self.cli, req).await
    }

    async fn close_workspace(&self, workspace_ref: &str) -> Result<(), LaunchError> {
        close::close_workspace(&self.cli, workspace_ref).await
    }

    async fn find_by_session(&self, session: Uuid) -> Result<Option<Surface>, LaunchError> {
        terminal::find_by_session(&self.cli, session).await
    }

    async fn close(&self, surface: &Surface, wait: Duration) -> Result<CloseOutcome, LaunchError> {
        close::close(&self.cli, surface, wait).await
    }

    async fn list(&self) -> Result<Vec<Surface>, LaunchError> {
        terminal::list(&self.cli).await
    }

    async fn send_prompt(&self, name: &str, text: &str, wait_ms: u64) -> Result<bool, LaunchError> {
        prompt::deliver_by_name(&self.cli, name, text, wait_ms).await
    }
}
