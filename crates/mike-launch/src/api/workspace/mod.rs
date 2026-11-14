pub mod briefing;
pub mod heartbeat;
pub mod registration;
pub mod roster;
pub mod roster_poll;
pub mod seat_pane;

pub use roster_poll::RosterAnswer;

pub use briefing::{
    Briefing, BriefingBuildRequest, build_workspace_briefing, extract_worktree_name, protocol_text,
};
pub use heartbeat::{get_wake_pending, heartbeat};
pub use registration::{assign_session_project, end_registration, register_session};
pub use roster::{get_project_activity, get_project_roster, resolve_project_path};

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Serialize)]
pub struct WorkspaceErrorBody {
    pub error: &'static str,
    pub message: String,
}

fn workspace_error(status: StatusCode, error: &'static str, message: String) -> Response {
    (status, Json(WorkspaceErrorBody { error, message })).into_response()
}
