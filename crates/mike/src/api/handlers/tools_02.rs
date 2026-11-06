use super::*;

pub(crate) const TOOL_ERRORS_DEFAULT_LIMIT: usize = 100;
pub(crate) const TOOL_ERRORS_MAX_LIMIT: usize = 500;

#[derive(Debug, Serialize)]
pub(crate) struct ToolResultDto {
    pub(crate) tool_use_id: String,
    pub(crate) is_error: bool,
    pub(crate) content_sha256: String,
    pub(crate) content_bytes: u64,
    pub(crate) content: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) request_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct RequestToolResultsResponse {
    pub(crate) request_id: u64,
    pub(crate) total_count: usize,
    pub(crate) results: Vec<ToolResultDto>,
}

#[derive(Debug, Serialize)]
pub(crate) struct PairedToolCall {
    pub(crate) tool_id: String,
    pub(crate) name: String,
    pub(crate) input: serde_json::Value,
    pub(crate) ord: i32,
    pub(crate) result: Option<ToolResultDto>,
}

#[derive(Debug, Serialize)]
pub(crate) struct RequestToolCallsResponse {
    pub(crate) request_id: u64,
    pub(crate) total_count: usize,
    pub(crate) calls: Vec<PairedToolCall>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ToolErrorRow {
    pub(crate) result_id: i64,
    pub(crate) request_id: i64,
    pub(crate) session_id: String,
    pub(crate) project: Option<String>,
    pub(crate) tool_use_id: String,
    pub(crate) tool_name: Option<String>,
    pub(crate) created_at: String,
    pub(crate) content_sha256: Option<String>,
    pub(crate) content_bytes: Option<i64>,
    pub(crate) content: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct ToolErrorsResponse {
    pub(crate) total_count: i64,
    pub(crate) total_count_capped: bool,
    pub(crate) offset: usize,
    pub(crate) limit: usize,
    pub(crate) next_cursor: Option<String>,
    pub(crate) rows: Vec<ToolErrorRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) window_note: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct ToolErrorsParams {
    #[serde(default)]
    pub(crate) since: Option<String>,
    #[serde(default)]
    pub(crate) date_from: Option<String>,
    #[serde(default)]
    pub(crate) date_to: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<usize>,
    #[serde(default)]
    pub(crate) offset: Option<usize>,
    #[serde(default)]
    pub(crate) project: Option<String>,
    #[serde(default)]
    pub(crate) tool_name: Option<String>,
    #[serde(default)]
    pub(crate) order: Option<SortOrder>,
    #[serde(default)]
    pub(crate) cursor: Option<String>,
}

pub(crate) async fn get_request_tool_results(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<RequestToolResultsResponse>, StatusCode> {
    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let request_id_i64 = id as i64;

    let exists = handle.request_exists(request_id_i64).await.map_err(|e| {
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

pub(crate) async fn get_request_tool_calls(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<RequestToolCallsResponse>, StatusCode> {
    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let request_id_i64 = id as i64;

    let exists = handle.request_exists(request_id_i64).await.map_err(|e| {
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
pub(crate) struct TurnPairedToolCall {
    pub(crate) tool_id: String,
    pub(crate) name: String,
    pub(crate) input_bytes: i32,
    pub(crate) ord: i32,
    pub(crate) request_id: i64,
    pub(crate) has_result: bool,
    pub(crate) result_is_error: Option<bool>,
    pub(crate) result_content_bytes: Option<i64>,
    pub(crate) operation: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct TurnToolCallsResponse {
    pub(crate) turn_id: i64,
    pub(crate) total_count: usize,
    pub(crate) calls: Vec<TurnPairedToolCall>,
}

pub(crate) async fn get_turn_tool_calls(
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

    match handle.fetch_turn_bounds(turn_id, session_db_id).await {
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
pub(crate) struct TurnToolCallContentResponse {
    pub(crate) tool_id: String,
    pub(crate) input: serde_json::Value,
    pub(crate) result: Option<ToolResultDto>,
}
