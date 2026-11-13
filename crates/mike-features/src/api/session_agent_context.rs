use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::reminders::agent_lessons::agent_lessons_answer;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct AgentContextRequest {
    agent_type: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct AgentContextResponse {
    agent_id: Option<i64>,
    lesson_ids: Vec<i64>,
    text: Option<String>,
}

pub async fn post_session_agent_context(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<AgentContextRequest>,
) -> Result<Json<AgentContextResponse>, StatusCode> {
    if Uuid::parse_str(&id).is_err() {
        return Err(StatusCode::BAD_REQUEST);
    }
    crate::api::client_serves::require_client_serves(&state, "client_serves_agent_lessons")?;
    let snapshot = state.lesson_cache.load();
    let answer = agent_lessons_answer(&snapshot, &id, &request.agent_type);
    Ok(Json(AgentContextResponse {
        agent_id: answer.agent_id,
        lesson_ids: answer.lesson_ids,
        text: answer.text,
    }))
}

#[cfg(test)]
#[path = "session_agent_context_tests.rs"]
mod tests;
