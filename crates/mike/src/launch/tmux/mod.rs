mod channels_dialog;
mod cli;
mod close;
mod open;
mod probe;
mod prompt;
mod seat_line;
mod surface;
#[cfg(test)]
mod tests;
mod validate;

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use uuid::Uuid;

use super::backend::{
    BackendId, Capabilities, CloseOutcome, OpenRequest, ProbeReport, Surface, TerminalBackend,
    ValidationReport,
};
use super::error::LaunchError;
use cli::TmuxCli;

const MIN_TMUX_MINOR: u32 = 2;

pub struct TmuxBackend {
    cli: TmuxCli,
    capture_budget: Duration,
}

impl Default for TmuxBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl TmuxBackend {
    pub fn new() -> Self {
        Self::with_binary("tmux")
    }

    pub fn with_binary(bin: impl Into<PathBuf>) -> Self {
        Self {
            cli: TmuxCli::new(bin),
            capture_budget: validate::DEFAULT_CAPTURE_BUDGET,
        }
    }

    #[cfg(test)]
    fn with_binary_and_capture_budget(bin: impl Into<PathBuf>, capture_budget: Duration) -> Self {
        Self {
            cli: TmuxCli::new(bin),
            capture_budget,
        }
    }
}

#[async_trait]
impl TerminalBackend for TmuxBackend {
    fn id(&self) -> BackendId {
        BackendId::Tmux
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            placement_workspace: false,
            placement_tab: true,
            placement_split: true,
            agent_state: false,
            prompt_in_argv: false,
            close_by_session: true,
            events: false,
        }
    }

    async fn probe(&self) -> ProbeReport {
        probe::probe(&self.cli).await
    }

    async fn validate(&self) -> ValidationReport {
        validate::validate(&self.cli, self.capture_budget).await
    }

    async fn open(&self, req: &OpenRequest) -> Result<Surface, LaunchError> {
        open::open(&self.cli, req).await
    }

    async fn find_by_session(&self, session: Uuid) -> Result<Option<Surface>, LaunchError> {
        find_by_session(&self.cli, session).await
    }

    async fn close(&self, surface: &Surface, wait: Duration) -> Result<CloseOutcome, LaunchError> {
        close::close(&self.cli, surface, wait).await
    }

    async fn list(&self) -> Result<Vec<Surface>, LaunchError> {
        list(&self.cli).await
    }
}

async fn find_by_session(cli: &TmuxCli, session: Uuid) -> Result<Option<Surface>, LaunchError> {
    let panes = surface::all_mike_panes(cli).await?;
    if let Some(pane) = panes.iter().find(|p| p.session_uuid == Some(session)) {
        let pid = surface::foreground_child_pid(cli, &pane.pane_id).await;
        return Ok(Some(surface::surface_from_pane(
            pane, pid, "existing", true,
        )));
    }
    let name = surface::session_name(session);
    if let Some(pane) = panes.iter().find(|p| p.session == name) {
        let pid = surface::foreground_child_pid(cli, &pane.pane_id).await;
        return Ok(Some(surface::surface_from_pane(
            pane, pid, "existing", true,
        )));
    }
    Ok(None)
}

async fn list(cli: &TmuxCli) -> Result<Vec<Surface>, LaunchError> {
    let panes = surface::all_mike_panes(cli).await?;
    let mut out = Vec::with_capacity(panes.len());
    for pane in panes.iter().filter(|p| p.current_command == "claude") {
        let pid = surface::foreground_child_pid(cli, &pane.pane_id).await;
        out.push(surface::surface_from_pane(pane, pid, "existing", true));
    }
    Ok(out)
}
