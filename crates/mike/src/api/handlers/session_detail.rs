use super::*;

mod get_session;
mod loaded_mods;
mod runtime_detail;
pub(crate) use get_session::get_session;

#[derive(Serialize)]
pub(crate) struct SessionDetail {
    pub(crate) observation: Option<crate::store::persistence::runtime::SessionObservationRow>,
    pub(crate) runtime_items: Option<runtime_detail::ItemPage>,
    pub(crate) summary: SessionSummary,
    pub(crate) requests: Vec<RequestSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) total_requests: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) offset: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) limit: Option<usize>,
    pub(crate) prompt_roster: Vec<PromptRosterEntry>,
    pub(crate) prompt_roster_truncated: bool,
    pub(crate) requests_truncated: bool,
    pub(crate) model_breakdown: Vec<ModelBreakdownEntry>,
    pub(crate) token_usage: SessionTokenUsageDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) quality: Option<SessionQualityDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) loaded_mods: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub(crate) struct PromptRosterEntry {
    pub(crate) prompt_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) prompt_composition_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) label: Option<String>,
    pub(crate) first_request_id: u64,
    pub(crate) occurrence_count: u64,
}

#[derive(Serialize)]
pub(crate) struct ModelBreakdownEntry {
    pub(crate) model: String,
    pub(crate) count: u64,
}

#[derive(Serialize)]
pub(crate) struct ModelTokenUsageEntry {
    pub(crate) model: String,
    pub(crate) response_count: u64,
    pub(crate) input_tokens: u64,
    pub(crate) output_tokens: u64,
    pub(crate) cache_read_tokens: u64,
    pub(crate) cache_write_tokens: u64,
}

#[derive(Serialize, Default)]
pub(crate) struct SessionTokenUsageDto {
    pub(crate) input_tokens: u64,
    pub(crate) output_tokens: u64,
    pub(crate) cache_read_tokens: u64,
    pub(crate) cache_write_tokens: u64,
    pub(crate) response_count: u64,
    pub(crate) by_model: Vec<ModelTokenUsageEntry>,
}

impl SessionTokenUsageDto {
    pub(crate) fn from_rows(
        rows: Vec<crate::store::persistence::rehydrate::SessionModelTokenUsageRow>,
    ) -> Self {
        let mut dto = Self::default();
        dto.by_model = rows
            .into_iter()
            .map(|r| {
                let entry = ModelTokenUsageEntry {
                    model: r.model,
                    response_count: r.response_count.max(0) as u64,
                    input_tokens: r.input_tokens.max(0) as u64,
                    output_tokens: r.output_tokens.max(0) as u64,
                    cache_read_tokens: r.cache_read_tokens.max(0) as u64,
                    cache_write_tokens: r.cache_write_tokens.max(0) as u64,
                };
                dto.input_tokens += entry.input_tokens;
                dto.output_tokens += entry.output_tokens;
                dto.cache_read_tokens += entry.cache_read_tokens;
                dto.cache_write_tokens += entry.cache_write_tokens;
                dto.response_count += entry.response_count;
                entry
            })
            .collect();
        dto
    }
}

#[derive(Serialize)]
pub(crate) struct SessionRequestsPage {
    pub(crate) session_id: String,
    pub(crate) requests: Vec<RequestSummary>,
    pub(crate) total_requests: usize,
    pub(crate) offset: usize,
    pub(crate) limit: usize,
}

pub(crate) async fn get_session_requests(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<SessionDetailParams>,
) -> Result<Json<SessionRequestsPage>, StatusCode> {
    if uuid::Uuid::parse_str(&id).is_err() {
        return Err(StatusCode::NOT_FOUND);
    }
    let offset = params.offset.unwrap_or(0);
    let limit = params
        .limit
        .unwrap_or(SESSION_DETAIL_DEFAULT_LIMIT)
        .clamp(1, SESSION_DETAIL_MAX_LIMIT);

    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let session_db_id = handle
        .resolve_session_db_id(&id)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, session_id = %id, "get_session_requests: session_db_id resolve failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let (requests, total_requests) = handle
        .fetch_session_request_summaries_paginated(
            session_db_id,
            offset,
            limit,
            params.order.unwrap_or_default(),
        )
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, session_id = %id, "get_session_requests: DB fetch failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let requests = requests.into_iter().map(RequestSummary::from).collect();

    Ok(Json(SessionRequestsPage {
        session_id: id,
        requests,
        total_requests,
        offset,
        limit,
    }))
}
