use super::*;

#[derive(Debug, Serialize)]
pub(crate) struct SessionHead {
    pub(crate) session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) last_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) newest_request_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) newest_request_number: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) originating_operator_prompt: Option<String>,
    pub(crate) prompt_kind_counts: BTreeMap<String, i64>,
}

pub(crate) async fn get_session_head(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SessionHead>, StatusCode> {
    if uuid::Uuid::parse_str(&id).is_err() {
        return Err(StatusCode::NOT_FOUND);
    }
    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let session_db_id = handle
        .resolve_session_db_id(&id)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, session_id = %id, "get_session_head: resolve failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    let row = handle
        .fetch_session_head(session_db_id)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, session_id = %id, "get_session_head: DB query failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    let prompt_kind_counts = handle
        .fetch_session_prompt_kind_counts(session_db_id)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, session_id = %id, "get_session_head: prompt_kind_counts query failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .into_iter()
        .map(|r| (r.kind_code, r.turn_count))
        .collect();
    Ok(Json(SessionHead {
        session_id: row.session_id,
        last_status: row.last_status,
        newest_request_id: row.newest_request_id.map(|n| n as u64),
        newest_request_number: row.newest_request_number.map(|n| n as u64),
        originating_operator_prompt: row.originating_operator_prompt,
        prompt_kind_counts,
    }))
}

fn validate_session_search_len(search: Option<&str>) -> Option<Response> {
    let s = search?;
    if s.chars().count() >= 2 {
        return None;
    }
    Some(
        (
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"error": "search must be at least 2 characters"})),
        )
            .into_response(),
    )
}

async fn run_list_sessions_query(
    handle: &crate::store::persistence::PersistHandle,
    params: &ListParams,
    offset: usize,
    limit: usize,
    search: Option<&str>,
    window: &crate::web_base::search_window::SearchWindow,
) -> Result<
    Vec<crate::store::persistence::rehydrate::SessionSummaryRow>,
    crate::store::persistence::PersistError,
> {
    let project_filter = params
        .project
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let listing_window = crate::store::persistence::listings::ListingWindow {
        order: params.order.unwrap_or_default(),
        date_from: window.effective_from,
        date_to: window_upper_bound(window),
    };
    if params.min_request_count.is_some() || project_filter.is_some() {
        handle
            .list_sessions_paginated_filtered(
                offset,
                limit,
                &crate::store::persistence::listings::SessionListFilters {
                    search,
                    min_request_count: params.min_request_count,
                    project: project_filter,
                },
                &listing_window,
            )
            .await
    } else {
        handle
            .list_sessions_paginated_with_search(offset, limit, search, &listing_window)
            .await
    }
}

fn build_session_list_response(
    state: &AppState,
    header_map: HeaderMap,
    rows: Vec<crate::store::persistence::rehydrate::SessionSummaryRow>,
    field_list: &Option<Vec<String>>,
) -> Response {
    let busy_ids = state.in_flight.busy_session_ids();
    let summaries: Vec<SessionSummary> = rows
        .iter()
        .map(|row| {
            let mut summary = SessionSummary::from(row);
            summary.busy = summary.busy || busy_ids.contains(&summary.id);
            summary
        })
        .collect();
    let body: Vec<serde_json::Value> = match field_list {
        Some(fields) => summaries
            .iter()
            .map(|s| project_session_summary_fields(s, fields))
            .collect(),
        None => summaries
            .iter()
            .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
            .collect(),
    };
    (header_map, axum::Json(body)).into_response()
}

pub(crate) async fn list_sessions(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<impl axum::response::IntoResponse, Response> {
    let field_list: Option<Vec<String>> = params.fields.as_deref().map(|raw| {
        raw.split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect()
    });

    let ids_filter = params
        .ids
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(raw_ids) = ids_filter {
        return list_sessions_by_ids_response(&state, raw_ids, &field_list).await;
    }

    let limit = params
        .limit
        .unwrap_or(SESSIONS_LIST_DEFAULT_LIMIT)
        .min(SESSIONS_LIST_MAX_LIMIT);
    let offset = params.offset.unwrap_or(0);
    let search = params
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(resp) = validate_session_search_len(search) {
        return Err(resp);
    }

    let window = crate::web_base::search_window::resolve_window(
        params.date_from.as_deref(),
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

    let header_map = window_note_headers(&window);
    let Some(handle) = state.persist() else {
        return Ok((header_map, axum::Json(Vec::<SessionSummary>::new())).into_response());
    };

    match run_list_sessions_query(handle, &params, offset, limit, search, &window).await {
        Ok(rows) => Ok(build_session_list_response(
            &state,
            header_map,
            rows,
            &field_list,
        )),
        Err(e) => {
            tracing::warn!(
                error = %e,
                "list_sessions: DB query failed"
            );
            Err((StatusCode::SERVICE_UNAVAILABLE, "database unavailable").into_response())
        }
    }
}

pub(crate) async fn list_sessions_by_ids_response(
    state: &AppState,
    raw_ids: &str,
    field_list: &Option<Vec<String>>,
) -> Result<Response, Response> {
    let raw_entries: Vec<&str> = raw_ids
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if raw_entries.len() > SESSIONS_IDS_MAX {
        return Err((
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({
                "error": format!(
                    "ids: at most {SESSIONS_IDS_MAX} ids allowed, got {}",
                    raw_entries.len()
                )
            })),
        )
            .into_response());
    }

    let mut parsed: Vec<uuid::Uuid> = Vec::with_capacity(raw_entries.len());
    for entry in &raw_entries {
        match entry.parse::<uuid::Uuid>() {
            Ok(id) => parsed.push(id),
            Err(_) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    axum::Json(serde_json::json!({
                        "error": format!("ids: invalid uuid '{entry}'")
                    })),
                )
                    .into_response());
            }
        }
    }
    let mut seen = std::collections::HashSet::with_capacity(parsed.len());
    parsed.retain(|id| seen.insert(*id));

    let Some(handle) = state.persist() else {
        return Ok((StatusCode::OK, axum::Json(Vec::<SessionSummary>::new())).into_response());
    };

    let rows = match handle.list_sessions_by_ids(&parsed).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(error = %e, "list_sessions(ids=): DB query failed");
            return Err((StatusCode::SERVICE_UNAVAILABLE, "database unavailable").into_response());
        }
    };

    let busy_ids = state.in_flight.busy_session_ids();
    let summaries: Vec<SessionSummary> = rows
        .iter()
        .map(|row| {
            let mut summary = SessionSummary::from(row);
            summary.busy = summary.busy || busy_ids.contains(&summary.id);
            summary
        })
        .collect();

    let body: Vec<serde_json::Value> = match field_list {
        Some(fields) => summaries
            .iter()
            .map(|s| project_session_summary_fields(s, fields))
            .collect(),
        None => summaries
            .iter()
            .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
            .collect(),
    };
    Ok((StatusCode::OK, axum::Json(body)).into_response())
}

pub(crate) fn project_session_summary_fields(
    summary: &SessionSummary,
    fields: &[String],
) -> serde_json::Value {
    let full = serde_json::to_value(summary).unwrap_or(serde_json::Value::Null);
    let Some(obj) = full.as_object() else {
        return full;
    };
    let mut projected = serde_json::Map::with_capacity(fields.len());
    for f in fields {
        if let Some(v) = obj.get(f.as_str()) {
            projected.insert(f.clone(), v.clone());
        }
    }
    serde_json::Value::Object(projected)
}

pub(crate) const SESSIONS_LIST_DEFAULT_LIMIT: usize = 50;
pub(crate) const SESSIONS_LIST_MAX_LIMIT: usize = 200;

pub(crate) const SESSIONS_IDS_MAX: usize = 200;
