pub mod browser_guard;
pub mod channels_dialog;
pub mod cli;
pub mod close;
pub mod display;
pub mod open;
pub mod pane;
pub mod probe;
pub mod prompt;
pub mod run_layout;
pub mod surface;
#[cfg(test)]
mod tests;
pub mod validate;

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use uuid::Uuid;

use super::backend::{
    BackendId, Capabilities, CloseOutcome, Notice, OpenRequest, PaneRequest, ProbeReport, Surface,
    TerminalBackend, ValidationReport,
};
use super::error::LaunchError;
use cli::HerdrCli;

pub const HERDR_PROTOCOL: u32 = 22;

pub struct HerdrBackend {
    cli: HerdrCli,
}

impl Default for HerdrBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl HerdrBackend {
    pub fn new() -> Self {
        Self::with_binary("herdr")
    }

    pub fn with_binary(bin: impl Into<PathBuf>) -> Self {
        Self {
            cli: HerdrCli::new(bin),
        }
    }
}

#[async_trait]
impl TerminalBackend for HerdrBackend {
    fn id(&self) -> BackendId {
        BackendId::Herdr
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            placement_workspace: true,
            placement_tab: true,
            placement_split: true,
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
        validate::validate(&self.cli).await
    }

    async fn open(&self, req: &OpenRequest) -> Result<Surface, LaunchError> {
        open::open(&self.cli, req).await
    }

    async fn open_pane(&self, req: &PaneRequest) -> Result<Surface, LaunchError> {
        pane::open(&self.cli, req).await
    }

    async fn report_pane(&self, surface_ref: &str, values: &[(String, String)]) {
        display::report(&self.cli, "pane", surface_ref, values).await;
    }

    async fn report_workspace(&self, workspace_ref: &str, values: &[(String, String)]) {
        display::report(&self.cli, "workspace", workspace_ref, values).await;
    }

    async fn notify(&self, notice: &Notice) {
        display::notify(&self.cli, notice).await;
    }

    async fn close_workspace(&self, workspace_ref: &str) -> Result<(), LaunchError> {
        close::close_workspace(&self.cli, workspace_ref).await
    }

    async fn find_by_session(&self, session: Uuid) -> Result<Option<Surface>, LaunchError> {
        surface::find_by_session(&self.cli, session).await
    }

    async fn close(&self, surface: &Surface, wait: Duration) -> Result<CloseOutcome, LaunchError> {
        close::close(&self.cli, surface, wait).await
    }

    async fn list(&self) -> Result<Vec<Surface>, LaunchError> {
        surface::list(&self.cli).await
    }

    async fn send_prompt(&self, name: &str, text: &str, wait_ms: u64) -> Result<bool, LaunchError> {
        let outcome = prompt::deliver(&self.cli, name, text, wait_ms).await?;
        Ok(outcome.is_confirmed())
    }
}
