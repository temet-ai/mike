use super::*;
use crate::store::persistence::steerer_repo::SteererPersist as _;

pub const TOOL_ERRORS_DEFAULT_LIMIT: usize = 100;
pub const TOOL_ERRORS_MAX_LIMIT: usize = 500;

#[derive(Debug, Serialize)]
pub struct ToolResultDto {
    pub tool_use_id: String,
    pub is_error: bool,
    pub content_sha256: String,
    pub content_bytes: u64,
    pub content: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct RequestToolResultsResponse {
    pub request_id: u64,
    pub total_count: usize,
    pub results: Vec<ToolResultDto>,
}

#[derive(Debug, Serialize)]
pub struct PairedToolCall {
    pub tool_id: String,
    pub name: String,
    pub input: serde_json::Value,
    pub ord: i32,
    pub result: Option<ToolResultDto>,
}

#[derive(Debug, Serialize)]
pub struct RequestToolCallsResponse {
    pub request_id: u64,
    pub total_count: usize,
    pub calls: Vec<PairedToolCall>,
}

#[derive(Debug, Serialize)]
pub struct ToolErrorRow {
    pub result_id: i64,
    pub request_id: i64,
    pub session_id: String,
    pub project: Option<String>,
    pub tool_use_id: String,
    pub tool_name: Option<String>,
    pub created_at: String,
    pub content_sha256: Option<String>,
    pub content_bytes: Option<i64>,
    pub content: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct ToolErrorsResponse {
    pub total_count: i64,
    pub total_count_capped: bool,
    pub offset: usize,
    pub limit: usize,
    pub next_cursor: Option<String>,
    pub rows: Vec<ToolErrorRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_note: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ToolErrorsParams {
    #[serde(default)]
    pub since: Option<String>,
    #[serde(default)]
    pub date_from: Option<String>,
    #[serde(default)]
    pub date_to: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub order: Option<SortOrder>,
    #[serde(default)]
    pub cursor: Option<String>,
}

pub async fn get_request_tool_results(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<RequestToolResultsResponse>, StatusCode> {
    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let request_id_i64 = id as i64;

    let exists = handle
        .steerer()
        .request_exists(request_id_i64)
        .await
        .map_err(|e| {
            tracing::warn!(
                request_id = id,
                error = %e,
                "get_request_tool_results: existence check failed"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if !exists {
        return Err(StatusCode::NOT_FOUND);
    }

    let rows = handle
        .steerer()
        .fetch_tool_results_for_request(request_id_i64)
        .await
        .map_err(|e| {
            tracing::warn!(
                request_id = id,
                error = %e,
                "get_request_tool_results: page query failed"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let results: Vec<ToolResultDto> = rows
        .into_iter()
        .map(|r| ToolResultDto {
            tool_use_id: r.tool_use_id,
            is_error: r.is_error,
            content_sha256: r.content_sha256,
            content_bytes: r.content_bytes.max(0) as u64,
            content: r.content,
            request_id: None,
        })
        .collect();
    let total_count = results.len();
    Ok(Json(RequestToolResultsResponse {
        request_id: id,
        total_count,
        results,
    }))
}

pub async fn get_request_tool_calls(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<RequestToolCallsResponse>, StatusCode> {
    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let request_id_i64 = id as i64;

    let exists = handle
        .steerer()
        .request_exists(request_id_i64)
        .await
        .map_err(|e| {
            tracing::warn!(
                request_id = id,
                error = %e,
                "get_request_tool_calls: existence check failed"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if !exists {
        return Err(StatusCode::NOT_FOUND);
    }

    let rows = handle
        .steerer()
        .fetch_request_tool_calls_paired(request_id_i64)
        .await
        .map_err(|e| {
            tracing::warn!(
                request_id = id,
                error = %e,
                "get_request_tool_calls: page query failed"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let calls: Vec<PairedToolCall> = rows
        .into_iter()
        .map(|r| PairedToolCall {
            tool_id: r.tool_id,
            name: r.tool_name,
            input: r.input.unwrap_or(serde_json::Value::Null),
            ord: r.ord,
            result: r.result_tool_use_id.map(|tool_use_id| ToolResultDto {
                tool_use_id,
                is_error: r.result_is_error.unwrap_or(false),
                content_sha256: r.result_content_sha256.unwrap_or_default(),
                content_bytes: r.result_content_bytes.unwrap_or(0).max(0) as u64,
                content: r.result_content.unwrap_or(serde_json::Value::Null),
                request_id: None,
            }),
        })
        .collect();
    let total_count = calls.len();
    Ok(Json(RequestToolCallsResponse {
        request_id: id,
        total_count,
        calls,
    }))
}

#[derive(Debug, Serialize)]
pub struct TurnPairedToolCall {
    pub tool_id: String,
    pub name: String,
    pub input_bytes: i32,
    pub ord: i32,
    pub request_id: i64,
    pub has_result: bool,
    pub result_is_error: Option<bool>,
    pub result_content_bytes: Option<i64>,
    pub operation: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TurnToolCallsResponse {
    pub turn_id: i64,
    pub total_count: usize,
    pub calls: Vec<TurnPairedToolCall>,
}

pub async fn get_turn_tool_calls(
    State(state): State<AppState>,
    Path((session_id, turn_id)): Path<(String, i64)>,
) -> Result<Json<TurnToolCallsResponse>, Response> {
    if uuid::Uuid::parse_str(&session_id).is_err() {
        return Err((StatusCode::NOT_FOUND, "unknown session").into_response());
    }
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };

    let session_db_id = match handle.resolve_session_db_id(&session_id).await {
        Ok(Some(db_id)) => db_id,
        Ok(None) => {
            return Err((StatusCode::NOT_FOUND, "unknown session").into_response());
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                session_id = %session_id,
                "get_turn_tool_calls: session resolve failed"
            );
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "turn tool-calls query failed",
            )
                .into_response());
        }
    };

    match handle
        .steerer()
        .fetch_turn_bounds(turn_id, session_db_id)
        .await
    {
        Ok(Some(_)) => {}
        Ok(None) => {
            return Err((StatusCode::NOT_FOUND, "unknown turn").into_response());
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                turn_id,
                session_id = %session_id,
                "get_turn_tool_calls: turn existence check failed"
            );
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "turn tool-calls query failed",
            )
                .into_response());
        }
    };

    let rows = handle
        .steerer()
        .fetch_turn_tool_calls_paired(turn_id, session_db_id)
        .await
        .map_err(|e| {
            tracing::warn!(
                error = %e,
                turn_id,
                session_id = %session_id,
                "get_turn_tool_calls: page query failed"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "turn tool-calls query failed",
            )
                .into_response()
        })?;

    let calls: Vec<TurnPairedToolCall> = rows
        .into_iter()
        .map(|r| TurnPairedToolCall {
            tool_id: r.tool_id,
            name: r.tool_name,
            input_bytes: r.input_bytes.unwrap_or(0),
            ord: r.ord,
            request_id: r.request_id,
            has_result: r.has_result,
            result_is_error: r.result_is_error,
            result_content_bytes: r.result_content_bytes,
            operation: r.operation,
        })
        .collect();
    let total_count = calls.len();
    Ok(Json(TurnToolCallsResponse {
        turn_id,
        total_count,
        calls,
    }))
}

#[derive(Debug, Serialize)]
pub struct TurnToolCallContentResponse {
    pub tool_id: String,
    pub input: serde_json::Value,
    pub result: Option<ToolResultDto>,
}
