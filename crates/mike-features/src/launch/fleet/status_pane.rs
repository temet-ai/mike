use std::time::Duration;

use serde_json::{Value, json};

use crate::launch::backend::TerminalBackend;
use crate::launch::{BackendId, Surface, backend_for};
use crate::state::AppState;
use crate::store::persistence::fleet::FleetRow;

pub const SEAT: &str = "status";
const CLOSE_WAIT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    pub surface: String,
    pub workspace: Option<String>,
    pub created: bool,
}

pub fn entry(surface: &Surface, problem: Option<String>) -> Value {
    json!({
        "seat": SEAT,
        "opened": true,
        "problem": problem,
        "surface": surface.surface_ref,
        "workspace": surface.workspace_ref,
        "created": !surface.reused,
    })
}

pub fn opened(start_report: Option<&Value>) -> Option<Opened> {
    let status = start_report?
        .as_array()?
        .iter()
        .find(|entry| entry.get("seat").and_then(Value::as_str) == Some(SEAT))?;
    if status.get("opened").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    Some(Opened {
        surface: status.get("surface")?.as_str()?.to_string(),
        workspace: status
            .get("workspace")
            .and_then(Value::as_str)
            .map(str::to_string),
        created: status
            .get("created")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

pub async fn close(state: &AppState, row: &FleetRow) {
    let Some(opened) = opened(row.start_report.as_ref()) else {
        return;
    };
    let Ok(id) = state.config.launch.backend.parse::<BackendId>() else {
        return;
    };
    let Some(backend) = backend_for(id) else {
        return;
    };
    close_with(backend.as_ref(), &row.slug, &opened).await;
}

pub async fn close_with(backend: &dyn TerminalBackend, slug: &str, opened: &Opened) {
    let surface = Surface {
        backend: backend.id(),
        workspace_ref: opened.workspace.clone(),
        tab_ref: None,
        surface_ref: opened.surface.clone(),
        pid: None,
        session_uuid: None,
        agent_state: None,
        cwd: None,
        actual_placement: String::new(),
        reused: !opened.created,
        delivered: None,
        confirmed: false,
        prompt_detail: None,
    };
    if let Err(err) = backend.close(&surface, CLOSE_WAIT).await {
        tracing::warn!(fleet = slug, pane = %opened.surface, error = %err, "fleet: the status pane could not be closed");
        return;
    }
    close_created_workspace(backend, slug, opened).await;
}

async fn close_created_workspace(backend: &dyn TerminalBackend, slug: &str, opened: &Opened) {
    let Some(workspace) = opened.workspace.as_deref().filter(|_| opened.created) else {
        return;
    };
    if let Err(err) = backend.close_workspace(workspace).await {
        tracing::warn!(fleet = slug, workspace, error = %err, "fleet: the run's workspace could not be closed");
    }
}

#[cfg(test)]
#[path = "status_pane_tests.rs"]
pub mod tests;
