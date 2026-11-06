use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::AppState;
use crate::reminders::client_context::{
    ContextBlock, findings_ready_within_budget, prompt_context,
};
use crate::reminders::git_read_hint;

#[derive(Debug, Deserialize)]
pub(crate) struct ContextRequest {
    project: String,
    #[serde(default)]
    serves: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ContextResponse {
    blocks: Vec<ContextBlock>,
    findings_ready: bool,
}

pub(crate) async fn post_session_context(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ContextRequest>,
) -> Result<Json<ContextResponse>, StatusCode> {
    let Ok(session) = Uuid::parse_str(&id) else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let (mut blocks, findings_ready) = tokio::join!(
        prompt_context(&state, session, &id, &request.project),
        findings_ready_within_budget(&state, &id),
    );
    if request.serves.iter().any(|s| s == "git_read_hint") {
        blocks.extend(git_read_hint::hand_over(&state, &id).await);
    }
    Ok(Json(ContextResponse {
        blocks,
        findings_ready,
    }))
}

#[cfg(test)]
mod tests;
