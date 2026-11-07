use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::config::AppState;
use crate::proxy_tools::answer::{ProxyToolCall, answer_tool_call};
use crate::proxy_tools::{client, declare, is_proxy_tool};

#[derive(Debug, Serialize)]
pub(crate) struct ToolsResponse {
    tools: Vec<Value>,
}

#[derive(Debug, Serialize)]
pub(crate) struct CallResponse {
    content: String,
}

fn parse_session(id: &str) -> Result<(), StatusCode> {
    Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| StatusCode::BAD_REQUEST)
}

pub(crate) async fn get_proxy_tools(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ToolsResponse>, StatusCode> {
    parse_session(&id)?;
    crate::api::client_serves::require_client_serves(&state, "client_serves_proxy_tools")?;
    client::mark_served(&id);
    Ok(Json(ToolsResponse {
        tools: declare::declared_tool_values(),
    }))
}

pub(crate) async fn post_proxy_tool(
    State(state): State<AppState>,
    Path((id, name)): Path<(String, String)>,
    input: Option<Json<Value>>,
) -> Result<Json<CallResponse>, StatusCode> {
    parse_session(&id)?;
    if !is_proxy_tool(&name) {
        return Err(StatusCode::NOT_FOUND);
    }
    let input_json = input.map(|Json(v)| v.to_string()).unwrap_or_default();
    let call = ProxyToolCall {
        id: String::new(),
        name,
        input_json,
    };
    let content = match session_project(&state, &id).await {
        Some(project) => answer_tool_call(&state, &call, &project).await,
        None => NO_PROJECT_ANSWER.to_owned(),
    };
    Ok(Json(CallResponse { content }))
}

const NO_PROJECT_ANSWER: &str = "This session is not assigned to a project, so mike has no \
                                 project roster to read for it.";

async fn session_project(state: &AppState, session: &str) -> Option<String> {
    let captured = state
        .store
        .read()
        .await
        .project_for_session(session)
        .unwrap_or_default();
    let snapshot = state.lesson_cache.load();
    let project = snapshot.project_for_session(session, &captured);
    (!project.is_empty()).then(|| project.to_owned())
}

pub(crate) async fn post_weasel_guard(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    parse_session(&id)?;
    crate::api::client_serves::require_client_serves(&state, "client_serves_weasel_guard")?;
    crate::weasel_intercept::evidence::mark_guarded_by_client(&id);
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests;
