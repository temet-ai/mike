mod briefing;
mod channel;
mod heartbeat;
mod registration;
mod roster;
mod roster_poll;
mod seat_pane;

pub(crate) use roster_poll::RosterAnswer;

pub(crate) use briefing::{
    Briefing, BriefingBuildRequest, build_workspace_briefing, extract_worktree_name, protocol_text,
};
pub(crate) use channel::{claim_channel_messages, release_channel_messages};
pub(crate) use heartbeat::{get_wake_pending, heartbeat};
pub(crate) use registration::{assign_session_project, end_registration, register_session};
pub(crate) use roster::{get_project_activity, get_project_roster, resolve_project_path};

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct WorkspaceErrorBody {
    pub(crate) error: &'static str,
    pub(crate) message: String,
}

fn workspace_error(status: StatusCode, error: &'static str, message: String) -> Response {
    (status, Json(WorkspaceErrorBody { error, message })).into_response()
}
