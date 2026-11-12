use super::*;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AgentDispatchDto {
    pub(crate) parent_request_id: i64,
    pub(crate) session_id: String,
    pub(crate) project_id: i64,
    pub(crate) created_at: String,
    pub(crate) tool_id: String,
    pub(crate) subagent_type: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) brief_char_len: i64,
    pub(crate) brief_sha256: Option<String>,
    pub(crate) ord: i32,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct AgentDispatchesParams {
    #[serde(default)]
    pub(crate) project: Option<String>,
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
    pub(crate) order: Option<SortOrder>,
}

pub(crate) const AGENT_DISPATCHES_DEFAULT_LIMIT: usize = 50;
pub(crate) const AGENT_DISPATCHES_MAX_LIMIT: usize = 500;

pub(crate) async fn list_agent_dispatches(
    State(state): State<AppState>,
    Query(params): Query<AgentDispatchesParams>,
) -> Result<impl axum::response::IntoResponse, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let limit = params
        .limit
        .unwrap_or(AGENT_DISPATCHES_DEFAULT_LIMIT)
        .clamp(1, AGENT_DISPATCHES_MAX_LIMIT) as i64;
    let offset = params.offset.unwrap_or(0) as i64;
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

    let filter = crate::store::persistence::AgentDispatchFilter {
        project: params.project,
        since: window.effective_from,
        until: window_upper_bound(&window),
        limit,
        offset,
        order: params.order.unwrap_or_default(),
    };
    let rows = handle
        .list_agent_dispatches_filtered(&filter)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "list_agent_dispatches: query failed");
            persist_error_response(e)
        })?;
    let dtos: Vec<AgentDispatchDto> = rows
        .into_iter()
        .map(|r| AgentDispatchDto {
            parent_request_id: r.parent_request_id,
            session_id: r.session_id,
            project_id: r.project_id,
            created_at: r.created_at.to_rfc3339(),
            tool_id: r.tool_id,
            subagent_type: r.subagent_type,
            description: r.description,
            brief_char_len: r.brief_char_len,
            brief_sha256: r.brief_sha256,
            ord: r.ord,
        })
        .collect();
    let header_map = window_note_headers(&window);
    Ok((header_map, axum::Json(dtos)).into_response())
}
