mod assemble;
pub(crate) mod dto;
mod profile;

#[cfg(test)]
mod tests;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use uuid::Uuid;

use crate::state::AppState;
use dto::SessionDeliveryDto;
use profile::{DELIVERY_TOGGLES, ProfileInputs};

const RECALL_TOOLS: [&str; 4] = [
    "mcp__mike__recall_project_context",
    "mcp__mike__recall_similar_turns",
    "mcp__mike__search_project_knowledge",
    "mcp__mike__read_project_knowledge",
];

pub(crate) async fn get_session_delivery(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SessionDeliveryDto>, StatusCode> {
    let Ok(session) = Uuid::parse_str(&id) else {
        return Err(StatusCode::NOT_FOUND);
    };
    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let toggle_keys: Vec<String> = DELIVERY_TOGGLES
        .iter()
        .map(|key| (*key).to_owned())
        .collect();
    let recall_tools: Vec<String> = RECALL_TOOLS.iter().map(|tool| (*tool).to_owned()).collect();
    let rows = handle
        .session_delivery(session, &toggle_keys, &recall_tools)
        .await
        .map_err(|error| {
            tracing::warn!(session_id = %session, error = %error, "get_session_delivery: DB query failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let current_toggles = state.feature_toggle_overlay.load();
    let current_settings = state.settings_overlay.load();
    let frozen = state
        .session_toggle_cache
        .get(&session.to_string())
        .map(|entry| entry.value().clone());
    let inputs = ProfileInputs {
        current_toggles: &current_toggles,
        current_settings: &current_settings,
        frozen: frozen.as_deref(),
    };
    Ok(Json(assemble::assemble(&rows, &inputs, chrono::Utc::now())))
}
