use super::*;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::steerer_repo::SteererPersist as _;

pub(crate) async fn get_turn_tool_call_content(
    State(state): State<AppState>,
    Path((session_id, turn_id, tool_id)): Path<(String, i64, String)>,
) -> Result<Json<TurnToolCallContentResponse>, Response> {
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
                "get_turn_tool_call_content: session resolve failed"
            );
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "turn tool-call content query failed",
            )
                .into_response());
        }
    };

    let row = handle
        .steerer()
        .fetch_turn_tool_call_content(turn_id, session_db_id, &tool_id)
        .await
        .map_err(|e| {
            tracing::warn!(
                error = %e,
                turn_id,
                session_id = %session_id,
                "get_turn_tool_call_content: query failed"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "turn tool-call content query failed",
            )
                .into_response()
        })?;

    let Some(row) = row else {
        return Err((StatusCode::NOT_FOUND, "unknown tool call").into_response());
    };

    let crate::store::persistence::turn_tool_call_reads::TurnToolCallContentRow {
        input,
        result_is_error,
        result_content,
        result_content_bytes,
        result_content_sha256,
    } = row;

    let result = result_is_error.map(|is_error| ToolResultDto {
        tool_use_id: tool_id.clone(),
        is_error,
        content_sha256: result_content_sha256.unwrap_or_default(),
        content_bytes: result_content_bytes.unwrap_or(0).max(0) as u64,
        content: result_content.unwrap_or(serde_json::Value::Null),
        request_id: None,
    });

    Ok(Json(TurnToolCallContentResponse {
        tool_id,
        input: input.unwrap_or(serde_json::Value::Null),
        result,
    }))
}

pub(crate) async fn list_tool_errors(
    State(state): State<AppState>,
    Query(params): Query<ToolErrorsParams>,
) -> Result<impl axum::response::IntoResponse, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let limit = params
        .limit
        .unwrap_or(TOOL_ERRORS_DEFAULT_LIMIT)
        .clamp(1, TOOL_ERRORS_MAX_LIMIT);
    let offset = params.offset.unwrap_or(0);

    let date_from_str = params.date_from.as_deref().or(params.since.as_deref());
    let window = crate::web_base::search_window::resolve_window(
        date_from_str,
        params.date_to.as_deref(),
        false,
    )
    .map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response()
    })?;
    let cursor = parse_cursor(params.cursor.as_deref())?;

    let filter = crate::store::persistence::tool_errors::ToolErrorFilter {
        project: params.project,
        tool_name: params.tool_name,
        since: window.effective_from,
        until: window_upper_bound(&window),
        limit: limit as i64,
        offset: offset as i64,
        cursor,
        order: params.order.unwrap_or_default(),
    };
    let page = handle.list_tool_errors(&filter).await.map_err(|e| {
        tracing::warn!(error = %e, "list_tool_errors: query failed");
        persist_error_response(e)
    })?;

    let dto_rows: Vec<ToolErrorRow> = page
        .rows
        .into_iter()
        .map(|row| ToolErrorRow {
            result_id: row.result_id,
            request_id: row.request_id,
            session_id: row.session_id,
            project: row.project_path,
            tool_use_id: row.tool_use_id,
            tool_name: row.tool_name,
            created_at: row.created_at.to_rfc3339(),
            content_sha256: row.content_sha256,
            content_bytes: row.content_bytes,
            content: row.content,
        })
        .collect();

    let window_note = if window.windowed {
        Some(window.widen_hint.clone())
    } else {
        None
    };

    let header_map = window_note_headers(&window);

    Ok((
        header_map,
        axum::Json(ToolErrorsResponse {
            total_count: page.total.count,
            total_count_capped: page.total.capped,
            offset: if cursor.is_some() { 0 } else { offset },
            limit,
            next_cursor: page.next_cursor.map(|c| c.encode()),
            rows: dto_rows,
            window_note,
        }),
    )
        .into_response())
}

#[derive(Debug, Serialize)]
pub(crate) struct ToolErrorFacets {
    pub(crate) projects: Vec<String>,
    pub(crate) tool_names: Vec<String>,
}

pub(crate) async fn list_tool_error_facets(
    State(state): State<AppState>,
) -> Result<Json<ToolErrorFacets>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let (projects, tool_names) = handle.list_tool_error_facets().await.map_err(|e| {
        tracing::warn!(error = %e, "list_tool_error_facets: query failed");
        persist_error_response(e)
    })?;
    Ok(Json(ToolErrorFacets {
        projects,
        tool_names,
    }))
}

pub(crate) const TOOL_USES_FEED_DEFAULT_LIMIT: usize = 200;
pub(crate) const TOOL_USES_FEED_MAX_LIMIT: usize = 1000;

#[derive(Debug, Deserialize, Default)]
pub(crate) struct ToolUsesFeedParams {
    #[serde(default)]
    pub(crate) since: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<usize>,
    #[serde(default)]
    pub(crate) offset: Option<usize>,
    #[serde(default)]
    pub(crate) project: Option<String>,
    #[serde(default)]
    pub(crate) tool_name: Option<String>,
    #[serde(default)]
    pub(crate) session_id: Option<String>,
    #[serde(default)]
    pub(crate) order: Option<SortOrder>,
    #[serde(default)]
    pub(crate) errors_only: Option<bool>,
    #[serde(default)]
    pub(crate) cursor: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ToolUseFeedRow {
    pub(crate) use_id: i64,
    pub(crate) wire_tool_use_id: String,
    pub(crate) request_id: i64,
    pub(crate) session_id: String,
    pub(crate) project: Option<String>,
    pub(crate) tool_name: String,
    pub(crate) created_at: String,
    pub(crate) ord: i32,
    pub(crate) input: serde_json::Value,
    pub(crate) file_path: Option<String>,
    pub(crate) is_error: bool,
    pub(crate) result_id: Option<i64>,
    pub(crate) error_content: Option<serde_json::Value>,
    pub(crate) error_content_bytes: Option<i64>,
    pub(crate) error_content_sha256: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ToolUsesFeedResponse {
    pub(crate) total_count: i64,
    pub(crate) total_count_capped: bool,
    pub(crate) offset: usize,
    pub(crate) limit: usize,
    pub(crate) next_cursor: Option<String>,
    pub(crate) rows: Vec<ToolUseFeedRow>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ToolUseFacets {
    pub(crate) projects: Vec<String>,
    pub(crate) tool_names: Vec<String>,
}

pub(crate) async fn list_tool_uses(
    State(state): State<AppState>,
    Query(params): Query<ToolUsesFeedParams>,
) -> Result<Json<ToolUsesFeedResponse>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let limit = params
        .limit
        .unwrap_or(TOOL_USES_FEED_DEFAULT_LIMIT)
        .clamp(1, TOOL_USES_FEED_MAX_LIMIT);
    let offset = params.offset.unwrap_or(0);
    let since: Option<chrono::DateTime<chrono::Utc>> = match params.since.as_deref() {
        Some(s) => match chrono::DateTime::parse_from_rfc3339(s) {
            Ok(dt) => Some(dt.with_timezone(&chrono::Utc)),
            Err(e) => {
                tracing::warn!(
                    since = %s,
                    error = %e,
                    "list_tool_uses: invalid `since`; ignoring"
                );
                None
            }
        },
        None => None,
    };
    let cursor = parse_cursor(params.cursor.as_deref())?;

    let filter = crate::store::persistence::tool_uses::ToolUsesFilter {
        project: params.project,
        tool_name: params.tool_name,
        session_id: params.session_id,
        since,
        errors_only: params.errors_only.unwrap_or(false),
        limit: limit as i64,
        offset: offset as i64,
        cursor,
        order: params.order.unwrap_or_default(),
    };
    let page = handle.launch().list_tool_uses(&filter).await.map_err(|e| {
        tracing::warn!(error = %e, "list_tool_uses: query failed");
        persist_error_response(e)
    })?;

    let rows: Vec<ToolUseFeedRow> = page
        .rows
        .into_iter()
        .map(|row| ToolUseFeedRow {
            use_id: row.use_id,
            wire_tool_use_id: row.wire_tool_use_id,
            request_id: row.request_id,
            session_id: row.session_id,
            project: row.project_path,
            tool_name: row.tool_name,
            created_at: row.created_at.to_rfc3339(),
            ord: row.ord,
            input: row.input.unwrap_or(serde_json::Value::Null),
            file_path: row.file_path,
            is_error: row.is_error,
            result_id: row.result_id,
            error_content: row.error_content,
            error_content_bytes: row.error_content_bytes,
            error_content_sha256: row.error_content_sha256,
        })
        .collect();

    Ok(Json(ToolUsesFeedResponse {
        total_count: page.total.count,
        total_count_capped: page.total.capped,
        offset: if cursor.is_some() { 0 } else { offset },
        limit,
        next_cursor: page.next_cursor.map(|c| c.encode()),
        rows,
    }))
}

pub(crate) struct InvalidCursor;

impl From<InvalidCursor> for Response {
    fn from(_: InvalidCursor) -> Self {
        (
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"error": "invalid cursor"})),
        )
            .into_response()
    }
}

pub(crate) fn parse_cursor(
    raw: Option<&str>,
) -> Result<Option<crate::store::persistence::keyset::KeysetCursor>, InvalidCursor> {
    let Some(raw) = raw.filter(|r| !r.is_empty()) else {
        return Ok(None);
    };
    crate::store::persistence::keyset::KeysetCursor::decode(raw)
        .map(Some)
        .ok_or(InvalidCursor)
}

pub(crate) async fn list_tool_use_facets(
    State(state): State<AppState>,
) -> Result<Json<ToolUseFacets>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let (projects, tool_names) = handle.launch().list_tool_use_facets().await.map_err(|e| {
        tracing::warn!(error = %e, "list_tool_use_facets: query failed");
        persist_error_response(e)
    })?;
    Ok(Json(ToolUseFacets {
        projects,
        tool_names,
    }))
}
