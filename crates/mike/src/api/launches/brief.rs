//! `POST /api/launches/{session}/brief`: the launcher's call, once a
//! spawned session's Remote Control stream is linked, to mail the session its
//! note-mode launch brief and push it over Remote Control.

use axum::Json;
use axum::extract::{Path, State};
use axum::response::Response;
use serde::Serialize;
use uuid::Uuid;

use crate::state::AppState;
use crate::web_base::{persist_error_response, persist_unavailable_response};

#[derive(Debug, Serialize)]
pub(crate) struct LaunchBriefDelivery {
    /// Brief mail waiting for delivery when the call arrived.
    pub(crate) pending: usize,
    /// How many of those were pushed over Remote Control. The rest stay
    /// queued and reach the session by injection on its next turn.
    pub(crate) pushed: usize,
}

pub(crate) async fn push_launch_brief(
    State(state): State<AppState>,
    Path(session): Path<Uuid>,
) -> Result<Json<LaunchBriefDelivery>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let briefs = handle
        .mail_launch_briefs(session)
        .await
        .map_err(persist_error_response)?;
    let pushed =
        crate::reminders::launch_brief::deliver_launch_briefs(&state, session, &briefs).await;
    Ok(Json(LaunchBriefDelivery {
        pending: briefs.len(),
        pushed,
    }))
}
