use super::*;
use crate::store::persistence::launch_repo::LaunchPersist as _;

use crate::store::persistence::keyset::{KeysetCursor, split_page};

#[derive(Serialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
pub(crate) struct RequestSummary {
    pub(crate) id: u64,
    pub(crate) session_id: String,
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) model: Option<String>,
    pub(crate) status: String,
    pub(crate) is_streaming: bool,
    pub(crate) message_count: usize,
    pub(crate) max_tokens: Option<u32>,
    pub(crate) created_at: String,
    pub(crate) completed_at: Option<String>,
    pub(crate) project: Option<String>,
    pub(crate) git_repo: Option<String>,
    pub(crate) human_prompt: String,
    pub(crate) turn_type: String,
    pub(crate) turn_origin: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) subagent_persona: Option<String>,
    pub(crate) tool_names: Vec<String>,
    pub(crate) response_tool_count: usize,
    pub(crate) response_tools_used: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) prompt_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) prompt_composition_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) predecessor_request_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) request_number: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) agent_instance_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) dispatch_request_id: Option<i64>,
}

impl From<&InterceptedRequest> for RequestSummary {
    fn from(req: &InterceptedRequest) -> Self {
        let mut tools_used: Vec<String> = Vec::new();
        for tc in &req.tool_calls {
            if !tools_used.iter().any(|existing| existing == &tc.name) {
                tools_used.push(tc.name.clone());
            }
        }

        Self {
            id: req.id.as_u64(),
            session_id: req.session_id.clone(),
            method: req.method.clone(),
            path: req.path.clone(),
            model: req.model.clone(),
            status: req.status.to_string(),
            is_streaming: req.is_streaming,
            message_count: req.message_count,
            max_tokens: req.params.max_tokens,
            created_at: req.created_at.to_rfc3339(),
            completed_at: req.completed_at.map(|t| t.to_rfc3339()),
            project: req.project.clone(),
            git_repo: req.git_repo.clone(),
            human_prompt: req.human_prompt.clone(),
            turn_type: req.turn_type.clone(),
            turn_origin: req.turn_origin.clone(),
            subagent_persona: req.subagent_persona.clone(),
            tool_names: req.tool_names.clone(),
            response_tool_count: req.tool_calls.len(),
            response_tools_used: tools_used,
            prompt_sha256: req.system_prompt.prompt_sha256.clone(),
            prompt_composition_id: req.system_prompt.composition_id.map(|u| u.to_string()),
            predecessor_request_id: None,
            request_number: None,
            agent_instance_id: req.agent_instance_id,
            dispatch_request_id: None,
        }
    }
}

impl From<crate::store::request_summary::RequestSummaryView> for RequestSummary {
    fn from(view: crate::store::request_summary::RequestSummaryView) -> Self {
        Self {
            id: view.id,
            session_id: view.session_id,
            method: view.method,
            path: view.path,
            model: view.model,
            status: view.status,
            is_streaming: view.is_streaming,
            message_count: view.message_count,
            max_tokens: view.max_tokens,
            created_at: view.created_at,
            completed_at: view.completed_at,
            project: view.project,
            git_repo: view.git_repo,
            human_prompt: view.human_prompt,
            turn_type: view.turn_type,
            turn_origin: view.turn_origin,
            subagent_persona: view.subagent_persona,
            tool_names: view.tool_names,
            response_tool_count: view.response_tool_count,
            response_tools_used: view.response_tools_used,
            prompt_sha256: view.prompt_sha256,
            prompt_composition_id: view.prompt_composition_id,
            predecessor_request_id: view.predecessor_request_id,
            request_number: view.request_number,
            agent_instance_id: view.agent_instance_id,
            dispatch_request_id: view.dispatch_request_id,
        }
    }
}

pub(crate) async fn list_requests(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<impl axum::response::IntoResponse, Response> {
    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);
    let order = params.order.unwrap_or_default();
    let sha_filter = params.prompt_sha256.as_deref().filter(|s| !s.is_empty());

    if let Some(sha) = sha_filter
        && !is_valid_prompt_sha256(sha)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            axum::Json(
                serde_json::json!({"error": "prompt_sha256 must be 64 lowercase hex characters"}),
            ),
        )
            .into_response());
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

    let cursor = parse_cursor(params.cursor.as_deref())?;
    let mut header_map = window_note_headers(&window);

    if let Some(handle) = state.persist() {
        match handle
            .launch()
            .list_requests_paginated(
                offset,
                limit,
                sha_filter,
                &crate::store::persistence::listings::ListingWindow {
                    order,
                    date_from: window.effective_from,
                    date_to: window_upper_bound(&window),
                },
                cursor,
            )
            .await
        {
            Ok((rows, next_cursor)) => {
                add_next_cursor_header(&mut header_map, next_cursor);
                let summaries: Vec<RequestSummary> =
                    rows.into_iter().map(RequestSummary::from).collect();
                return Ok((header_map, axum::Json(summaries)).into_response());
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "list_requests: DB query failed; falling back to in-memory walk"
                );
            }
        }
    }

    let from = window.effective_from;
    let to = window_upper_bound(&window);
    let store = state.store.read().await;
    let mut walk: Vec<&InterceptedRequest> = if let Some(sha) = sha_filter {
        store
            .recent(usize::MAX)
            .into_iter()
            .filter(|req| {
                req.system_prompt
                    .prompt_sha256
                    .as_deref()
                    .is_some_and(|cand| cand == sha)
                    && from.is_none_or(|f| req.created_at >= f)
                    && to.is_none_or(|t| req.created_at < t)
            })
            .collect()
    } else {
        store
            .recent(usize::MAX)
            .into_iter()
            .filter(|req| {
                from.is_none_or(|f| req.created_at >= f) && to.is_none_or(|t| req.created_at < t)
            })
            .collect()
    };
    if order == SortOrder::Asc {
        walk.reverse();
    }
    let (page, next_cursor) = page_in_memory(walk, order, cursor, offset, limit);
    add_next_cursor_header(&mut header_map, next_cursor);
    let summaries: Vec<RequestSummary> = page.into_iter().map(RequestSummary::from).collect();
    Ok((header_map, axum::Json(summaries)).into_response())
}

fn page_in_memory(
    mut walk: Vec<&InterceptedRequest>,
    order: SortOrder,
    cursor: Option<KeysetCursor>,
    offset: usize,
    limit: usize,
) -> (Vec<&InterceptedRequest>, Option<KeysetCursor>) {
    if let Some(after) = cursor {
        walk.retain(|req| {
            let key = (req.created_at, request_key_id(req));
            match order {
                SortOrder::Desc => key < (after.at, after.id),
                SortOrder::Asc => key > (after.at, after.id),
            }
        });
    }
    let skip = if cursor.is_some() { 0 } else { offset };
    let probed: Vec<&InterceptedRequest> = walk
        .into_iter()
        .skip(skip)
        .take(limit.saturating_add(1))
        .collect();
    split_page(probed, i64::try_from(limit).unwrap_or(i64::MAX), |req| {
        KeysetCursor {
            at: req.created_at,
            id: request_key_id(req),
        }
    })
}

fn request_key_id(req: &InterceptedRequest) -> i64 {
    i64::try_from(req.id.as_u64()).unwrap_or(i64::MAX)
}

fn add_next_cursor_header(headers: &mut axum::http::HeaderMap, next: Option<KeysetCursor>) {
    if let Some(next) = next
        && let Ok(value) = axum::http::HeaderValue::try_from(next.encode())
    {
        headers.insert("x-next-cursor", value);
    }
}

pub(crate) async fn get_request_summary(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<RequestSummary>, StatusCode> {
    let req = match load_request_from_db(&state, id).await? {
        Some(req) => req,
        None => return Err(StatusCode::NOT_FOUND),
    };
    let predecessor_request_id = if req.turn_origin == "primary" || req.turn_origin == "subagent" {
        match state.persist() {
            Some(handle) => {
                let session_db_id = handle
                    .resolve_session_db_id(&req.session_id)
                    .await
                    .map_err(|e| {
                        tracing::warn!(
                            error = %e,
                            request_id = id,
                            "get_request_summary: session_db_id resolution failed",
                        );
                        StatusCode::INTERNAL_SERVER_ERROR
                    })?;
                match session_db_id {
                    Some(db_id) => handle
                        .fetch_predecessor_request_id(db_id, req.created_at, req.agent_instance_id)
                        .await
                        .map_err(|e| {
                            tracing::warn!(
                                error = %e,
                                request_id = id,
                                "get_request_summary: predecessor lookup failed",
                            );
                            StatusCode::INTERNAL_SERVER_ERROR
                        })?,
                    None => None,
                }
            }
            None => None,
        }
    } else {
        None
    };
    let mut summary = RequestSummary::from(&req);
    summary.predecessor_request_id = predecessor_request_id;
    Ok(Json(summary))
}
