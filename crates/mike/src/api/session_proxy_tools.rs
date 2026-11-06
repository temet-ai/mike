use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::config::AppState;
use crate::proxy_tools::answer::{ProxyToolCall, answer_tool_call};
use crate::proxy_tools::{client, declare, is_proxy_tool};

#[derive(Debug, Serialize)]
pub(crate) struct ToolsResponse {
    tools: Vec<Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CallQuery {
    project: String,
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
    Query(query): Query<CallQuery>,
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
    let content = answer_tool_call(&state, &call, &query.project).await;
    Ok(Json(CallResponse { content }))
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
