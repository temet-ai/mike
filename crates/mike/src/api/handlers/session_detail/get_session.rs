use super::*;
use crate::store::persistence::eval_repo::EvalPersist as _;

struct SessionHeader {
    session_db_id: i64,
    summary: SessionSummary,
    etag_value: String,
}

async fn resolve_session_header(
    handle: &crate::store::persistence::PersistHandle,
    state: &AppState,
    id: &str,
) -> Result<Option<SessionHeader>, StatusCode> {
    let Some(session_db_id) = handle.resolve_session_db_id(id).await.map_err(|e| {
        tracing::warn!(error = %e, session_id = %id, "get_session: session_db_id resolve failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    else {
        return Ok(None);
    };
    let summary_row = handle
        .fetch_session_summary(session_db_id)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, session_id = %id, "get_session: DB summary fetch failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let Some(summary_row) = summary_row else {
        return Ok(None);
    };

    let mut summary = SessionSummary::from(&summary_row);
    summary.busy = compute_session_busy(state.in_flight.is_session_busy(id), &summary.last_status);

    let etag_value = session_etag(
        &summary.last_seen,
        summary.request_count,
        summary.busy,
        summary.session_judged_at.as_deref(),
    );

    Ok(Some(SessionHeader {
        session_db_id,
        summary,
        etag_value,
    }))
}

struct SessionAggregations {
    request_summaries: Vec<RequestSummary>,
    total: usize,
    prompt_roster: Vec<PromptRosterEntry>,
    prompt_roster_truncated: bool,
    model_breakdown: Vec<ModelBreakdownEntry>,
    token_usage: SessionTokenUsageDto,
    quality: Option<SessionQualityDto>,
    loaded_mods: Option<serde_json::Value>,
}

fn map_token_usage_result(
    result: Result<
        Vec<crate::store::persistence::rehydrate::SessionModelTokenUsageRow>,
        crate::store::persistence::PersistError,
    >,
    session_id: &str,
) -> SessionTokenUsageDto {
    match result {
        Ok(rows) => SessionTokenUsageDto::from_rows(rows),
        Err(e) => {
            tracing::warn!(
                session_id = %session_id,
                error = %e,
                "get_session: token usage fetch failed — reporting zeroed totals"
            );
            SessionTokenUsageDto::default()
        }
    }
}

fn map_quality_result(
    result: Result<
        Option<crate::store::persistence::judge::SessionQualityRow>,
        crate::store::persistence::PersistError,
    >,
    session_id: &str,
) -> Option<SessionQualityDto> {
    match result {
        Ok(Some(q)) => Some(SessionQualityDto::from(q)),
        Ok(None) => None,
        Err(e) => {
            tracing::warn!(
                session_id = %session_id,
                error = %e,
                "get_session: quality fetch failed — omitting from response"
            );
            None
        }
    }
}

async fn fetch_session_aggregations(
    handle: &crate::store::persistence::PersistHandle,
    id: &str,
    session_db_id: i64,
    effective_offset: usize,
    effective_limit: usize,
    order: SortOrder,
) -> Result<SessionAggregations, StatusCode> {
    let (
        request_summaries_result,
        roster_result,
        breakdown_result,
        token_usage_result,
        quality_result,
        loaded_mods,
    ) = tokio::join!(
        handle.fetch_session_request_summaries_paginated(
            session_db_id,
            effective_offset,
            effective_limit,
            order,
        ),
        handle.fetch_session_prompt_roster(session_db_id),
        handle.fetch_session_model_breakdown(session_db_id),
        handle.fetch_session_token_usage(session_db_id),
        handle.eval().fetch_session_quality(id),
        super::loaded_mods::fetch_loaded_mods(handle, id),
    );

    let (request_summaries, total) = request_summaries_result.map_err(|e| {
        tracing::warn!(error = %e, session_id = %id, "get_session: DB fallback requests failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let request_summaries = request_summaries
        .into_iter()
        .map(RequestSummary::from)
        .collect();
    let (roster_rows, prompt_roster_truncated) = roster_result.map_err(|e| {
        tracing::warn!(error = %e, session_id = %id, "get_session: DB fallback prompt roster failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let breakdown_rows = breakdown_result.map_err(|e| {
        tracing::warn!(error = %e, session_id = %id, "get_session: DB fallback model breakdown failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let token_usage = map_token_usage_result(token_usage_result, id);
    let quality = map_quality_result(quality_result, id);

    let prompt_roster: Vec<PromptRosterEntry> = roster_rows
        .into_iter()
        .map(|r| PromptRosterEntry {
            prompt_sha256: r.prompt_sha256,
            prompt_composition_id: r.composition_id.map(|u| u.to_string()),
            label: r.label,
            first_request_id: r.first_request_id.max(0) as u64,
            occurrence_count: r.occurrence_count.max(0) as u64,
        })
        .collect();
    let model_breakdown: Vec<ModelBreakdownEntry> = breakdown_rows
        .into_iter()
        .map(|r| ModelBreakdownEntry {
            model: r.model,
            count: r.count.max(0) as u64,
        })
        .collect();

    Ok(SessionAggregations {
        request_summaries,
        total,
        prompt_roster,
        prompt_roster_truncated,
        model_breakdown,
        token_usage,
        quality,
        loaded_mods,
    })
}

fn build_session_detail(
    summary: SessionSummary,
    aggregations: SessionAggregations,
    paginated: bool,
    effective_offset: usize,
    effective_limit: usize,
) -> SessionDetail {
    let SessionAggregations {
        request_summaries,
        total,
        prompt_roster,
        prompt_roster_truncated,
        model_breakdown,
        token_usage,
        quality,
        loaded_mods,
    } = aggregations;

    if paginated {
        SessionDetail {
            observation: None,
            runtime_items: None,
            summary,
            requests: request_summaries,
            total_requests: Some(total),
            offset: Some(effective_offset),
            limit: Some(effective_limit),
            prompt_roster,
            prompt_roster_truncated,
            requests_truncated: false,
            model_breakdown,
            token_usage,
            quality,
            loaded_mods,
        }
    } else {
        let requests_truncated = total > SESSION_DETAIL_MAX_LIMIT;
        SessionDetail {
            observation: None,
            runtime_items: None,
            summary,
            requests: request_summaries,
            total_requests: None,
            offset: None,
            limit: None,
            prompt_roster,
            prompt_roster_truncated,
            requests_truncated,
            model_breakdown,
            token_usage,
            quality,
            loaded_mods,
        }
    }
}

pub(crate) async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<SessionDetailParams>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    if uuid::Uuid::parse_str(&id).is_err() {
        return Err(StatusCode::NOT_FOUND);
    }
    let paginated = params.offset.is_some() || params.limit.is_some();
    let offset = params.offset.unwrap_or(0);
    let limit = params
        .limit
        .unwrap_or(SESSION_DETAIL_DEFAULT_LIMIT)
        .clamp(1, SESSION_DETAIL_MAX_LIMIT);

    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let Some(session_header) = resolve_session_header(handle, &state, &id).await? else {
        return Err(StatusCode::NOT_FOUND);
    };

    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|inm| if_none_match_contains(inm, &session_header.etag_value))
    {
        let mut not_modified = StatusCode::NOT_MODIFIED.into_response();
        if let Ok(v) = HeaderValue::from_str(&session_header.etag_value) {
            not_modified.headers_mut().insert(header::ETAG, v);
        }
        return Ok(not_modified);
    }

    let (effective_offset, effective_limit) = if paginated {
        (offset, limit)
    } else {
        (0usize, SESSION_DETAIL_MAX_LIMIT)
    };

    let aggregations = fetch_session_aggregations(
        handle,
        &id,
        session_header.session_db_id,
        effective_offset,
        effective_limit,
        params.order.unwrap_or_default(),
    )
    .await?;

    let mut detail = build_session_detail(
        session_header.summary,
        aggregations,
        paginated,
        effective_offset,
        effective_limit,
    );

    super::runtime_detail::attach(handle, &id, &mut detail, effective_offset, effective_limit)
        .await?;
    let mut response = Json(detail).into_response();
    if let Ok(v) = HeaderValue::from_str(&session_header.etag_value) {
        response.headers_mut().insert(header::ETAG, v);
    }
    Ok(response)
}
