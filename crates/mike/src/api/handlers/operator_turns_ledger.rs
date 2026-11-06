use super::*;

#[derive(Debug, Default, Deserialize)]
pub(crate) struct SessionOperatorTurnsParams {
    #[serde(default)]
    pub(crate) order: Option<SortOrder>,
    #[serde(default)]
    pub(crate) prompt_kind: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<i64>,
    #[serde(default)]
    pub(crate) offset: Option<i64>,
    #[serde(default)]
    pub(crate) fields: Option<String>,
}

pub(crate) async fn get_session_operator_turns(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Query(params): Query<SessionOperatorTurnsParams>,
) -> Result<Json<Vec<OperatorTurnDto>>, Response> {
    if uuid::Uuid::parse_str(&session_id).is_err() {
        return Err((StatusCode::NOT_FOUND, "unknown session").into_response());
    }
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let session_db_id = handle
        .resolve_session_db_id(&session_id)
        .await
        .map_err(|e| {
            tracing::warn!(session_id = %session_id, error = %e, "get_session_operator_turns: session_db_id resolution failed");
            persist_error_response(e)
        })?;
    let Some(db_id) = session_db_id else {
        return Err((StatusCode::NOT_FOUND, "unknown session").into_response());
    };
    let filter = crate::store::persistence::operator_turns::SessionOverviewFilter {
        prompt_kind: params.prompt_kind.clone(),
        limit: params.limit,
        offset: params.offset,
        omit_stop_response: false,
    };
    let rows = handle
        .list_for_session_bounded(db_id, params.order.unwrap_or_default(), &filter)
        .await
        .map(|(rows, _truncated)| rows)
        .map_err(|e| {
            tracing::warn!(
                session_id = %session_id,
                error = %e,
                "get_session_operator_turns: query failed"
            );
            persist_error_response(e)
        })?;
    let dtos: Vec<OperatorTurnDto> = rows.into_iter().map(OperatorTurnDto::from).collect();
    Ok(Json(dtos))
}

#[derive(Debug, Serialize)]
pub(crate) struct AskLedgerEntryDto {
    pub(crate) id: i64,
    pub(crate) operator_turn_id: Option<i64>,
    pub(crate) request_id: Option<i64>,
    pub(crate) kind: String,
    pub(crate) kind_source: String,
    pub(crate) keyword_kind: Option<String>,
    pub(crate) kind_updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) genuine: bool,
    pub(crate) content: String,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AskLedgerParams {
    #[serde(default)]
    pub(crate) limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AskLedgerResponse {
    pub(crate) session_id: String,
    pub(crate) entries: Vec<AskLedgerEntryDto>,
}

pub(crate) async fn get_session_ask_ledger(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Query(params): Query<AskLedgerParams>,
) -> Result<Json<AskLedgerResponse>, Response> {
    let Ok(session_uuid) = uuid::Uuid::parse_str(&session_id) else {
        return Err((StatusCode::NOT_FOUND, "unknown session").into_response());
    };
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let session_db_id = handle
        .resolve_session_db_id(&session_id)
        .await
        .map_err(|e| {
            tracing::warn!(session_id = %session_id, error = %e, "get_session_ask_ledger: session_db_id resolution failed");
            persist_error_response(e)
        })?;
    if session_db_id.is_none() {
        return Err((StatusCode::NOT_FOUND, "unknown session").into_response());
    }

    let limit = params.limit.unwrap_or(50).clamp(1, 500);
    let entries = handle
        .fetch_ask_ledger_inspection(session_uuid, limit)
        .await
        .map_err(|e| {
            tracing::warn!(session_id = %session_id, error = %e, "get_session_ask_ledger: query failed");
            persist_error_response(e)
        })?;

    Ok(Json(AskLedgerResponse {
        session_id,
        entries: entries
            .into_iter()
            .map(|e| AskLedgerEntryDto {
                id: e.id,
                operator_turn_id: e.operator_turn_id,
                request_id: e.request_id,
                kind: e.kind,
                kind_source: e.kind_source,
                keyword_kind: e.keyword_kind,
                kind_updated_at: e.kind_updated_at,
                genuine: e.genuine,
                content: e.content,
                created_at: e.created_at,
            })
            .collect(),
    }))
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct AcceptanceCriterionDto {
    pub(crate) ord: i32,
    pub(crate) criterion: String,
    pub(crate) provenance: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) operator_span: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct AcceptanceCriteriaMapDto {
    pub(crate) version: i32,
    pub(crate) ask_entry_count: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) output_tokens: Option<i32>,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) criteria: Vec<AcceptanceCriterionDto>,
}

pub(crate) async fn get_session_acceptance_criteria(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<AcceptanceCriteriaMapDto>>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let session_db_id = handle
        .resolve_session_db_id(&session_id)
        .await
        .map_err(|e| {
            tracing::warn!(session_id = %session_id, error = %e, "get_session_acceptance_criteria: session_db_id resolution failed");
            persist_error_response(e)
        })?;
    let Some(session_db_id) = session_db_id else {
        return Err((StatusCode::NOT_FOUND, "unknown session").into_response());
    };

    let maps = handle
        .fetch_acceptance_criteria_history(session_db_id)
        .await
        .map_err(|e| {
            tracing::warn!(session_id = %session_id, error = %e, "get_session_acceptance_criteria: query failed");
            persist_error_response(e)
        })?;

    Ok(Json(
        maps.into_iter()
            .map(|m| AcceptanceCriteriaMapDto {
                version: m.version,
                ask_entry_count: m.ask_entry_count,
                model: m.model,
                output_tokens: m.output_tokens,
                created_at: m.created_at,
                criteria: m
                    .criteria
                    .into_iter()
                    .map(|c| AcceptanceCriterionDto {
                        ord: c.ord,
                        criterion: c.criterion,
                        provenance: c.provenance,
                        operator_span: c.operator_span,
                    })
                    .collect(),
            })
            .collect(),
    ))
}
