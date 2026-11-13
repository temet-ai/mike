use super::*;

#[derive(Debug, Default, Deserialize)]
pub(crate) struct OperatorTurnsListParams {
    #[serde(default)]
    pub(crate) project: Option<String>,
    #[serde(default)]
    pub(crate) session: Option<String>,
    #[serde(default)]
    pub(crate) since: Option<String>,
    #[serde(default)]
    pub(crate) date_from: Option<String>,
    #[serde(default)]
    pub(crate) date_to: Option<String>,
    #[serde(default)]
    pub(crate) outcome: Option<String>,
    #[serde(default)]
    pub(crate) subject_command: Option<String>,
    #[serde(default)]
    pub(crate) subject_agent: Option<String>,
    #[serde(default)]
    pub(crate) skill: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<usize>,
    #[serde(default)]
    pub(crate) offset: Option<usize>,
    #[serde(default)]
    pub(crate) order: Option<SortOrder>,
}

pub(crate) const OPERATOR_TURNS_DEFAULT_LIMIT: usize = 50;
pub(crate) const OPERATOR_TURNS_MAX_LIMIT: usize = 500;

pub(crate) async fn list_operator_turns(
    State(state): State<AppState>,
    Query(params): Query<OperatorTurnsListParams>,
) -> Result<impl axum::response::IntoResponse, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let limit = params
        .limit
        .unwrap_or(OPERATOR_TURNS_DEFAULT_LIMIT)
        .clamp(1, OPERATOR_TURNS_MAX_LIMIT) as i64;
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
    let correction = params.outcome.as_deref() == Some("correction");
    let outcome = match params.outcome.as_deref() {
        Some(o) => match o {
            "unknown" | "accepted" | "corrected" | "abandoned" | "shipped" => Some(o.to_string()),
            "correction" => None,
            _ => {
                tracing::warn!(outcome = %o, "list_operator_turns: invalid outcome, ignoring");
                None
            }
        },
        None => None,
    };
    let session_db_id = if let Some(ref uuid) = params.session {
        handle.resolve_session_db_id(uuid).await.map_err(|e| {
            tracing::warn!(error = %e, "list_operator_turns: session_db_id resolution failed");
            persist_error_response(e)
        })?
    } else {
        None
    };
    let filter = crate::store::persistence::operator_turns::ListTurnsFilter {
        project_path: params.project,
        session_db_id,
        since: window.effective_from,
        until: window_upper_bound(&window),
        outcome,
        correction,
        subject_command: params.subject_command,
        subject_agent: params.subject_agent,
        skill: params.skill,
        limit,
        offset,
        order: params.order.unwrap_or_default(),
    };
    let rows = handle.list_turns(&filter).await.map_err(|e| {
        tracing::warn!(error = %e, "list_operator_turns: query failed");
        persist_error_response(e)
    })?;
    let dtos: Vec<OperatorTurnDto> = rows.into_iter().map(OperatorTurnDto::from).collect();
    let header_map = window_note_headers(&window);
    Ok((header_map, axum::Json(dtos)).into_response())
}

pub(crate) async fn get_operator_turn(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<OperatorTurnDto>, StatusCode> {
    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let row = handle.get_by_id(id).await.map_err(|e| {
        tracing::warn!(turn_id = id, error = %e, "get_operator_turn: query failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    match row {
        Some(r) => Ok(Json(OperatorTurnDto::from(r))),
        None => Err(StatusCode::NOT_FOUND),
    }
}
