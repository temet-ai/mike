use super::*;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OperatorTurnDto {
    pub(crate) turn_id: i64,
    pub(crate) session_id: String,
    pub(crate) project_id: i64,
    pub(crate) start_request_id: i64,
    pub(crate) end_request_id: i64,
    pub(crate) start_request_number: i64,
    pub(crate) end_request_number: i64,
    pub(crate) user_prompt: String,
    pub(crate) final_assistant_text: String,
    pub(crate) tool_call_count: i32,
    pub(crate) tool_error_count: i32,
    pub(crate) files_touched: Vec<String>,
    pub(crate) tools_used: Vec<String>,
    pub(crate) agents_dispatched: Vec<String>,
    pub(crate) skills_used: Vec<String>,
    pub(crate) weasel_findings_count: i32,
    pub(crate) total_input_tokens: i64,
    pub(crate) total_output_tokens: i64,
    pub(crate) duration_ms: i64,
    pub(crate) stop_response: Option<String>,
    pub(crate) stop_reason: Option<String>,
    pub(crate) stop_request_id: Option<i64>,
    pub(crate) interrupted: bool,
    pub(crate) outcome: String,
    pub(crate) outcome_evidence: Option<String>,
    pub(crate) classified_at: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) prompt_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) subject_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) subject_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) model: Option<String>,
    pub(crate) outcome_bundle_id: Option<i64>,
    pub(crate) reaction: Option<String>,
    pub(crate) delivery: Option<String>,
    pub(crate) outcome_source: Option<String>,
    pub(crate) counts_as_correction: bool,
    pub(crate) live_verdict_id: Option<i64>,
    pub(crate) outcome_projection_version: Option<i16>,
    pub(crate) legacy: bool,
    pub(crate) pending: bool,
}

pub(crate) fn projection_flags(outcome_source: Option<&str>, outcome: &str) -> (bool, bool) {
    let unprojected = outcome_source.is_none();
    (
        unprojected && outcome != "unknown",
        unprojected && outcome == "unknown",
    )
}

impl From<crate::store::operator_turns::OperatorTurnRow> for OperatorTurnDto {
    fn from(r: crate::store::operator_turns::OperatorTurnRow) -> Self {
        let (legacy, pending) = projection_flags(r.outcome_source.as_deref(), &r.outcome);
        Self {
            turn_id: r.turn_id,
            session_id: r.session_id,
            project_id: r.project_id,
            start_request_id: r.start_request_id,
            end_request_id: r.end_request_id,
            start_request_number: r.start_request_number,
            end_request_number: r.end_request_number,
            user_prompt: r.user_prompt,
            final_assistant_text: r.final_assistant_text,
            tool_call_count: r.tool_call_count,
            tool_error_count: r.tool_error_count,
            files_touched: r.files_touched,
            tools_used: r.tools_used,
            agents_dispatched: r.agents_dispatched,
            skills_used: r.skills_used,
            weasel_findings_count: r.weasel_findings_count,
            total_input_tokens: r.total_input_tokens,
            total_output_tokens: r.total_output_tokens,
            duration_ms: r.duration_ms,
            stop_response: r.stop_response,
            stop_reason: r.stop_reason,
            stop_request_id: r.stop_request_id,
            interrupted: r.interrupted,
            outcome: r.outcome,
            outcome_evidence: r.outcome_evidence,
            classified_at: r.classified_at.map(|t| t.to_rfc3339()),
            created_at: r.created_at.to_rfc3339(),
            updated_at: r.updated_at.to_rfc3339(),
            prompt_kind: r.prompt_kind,
            subject_command: r.subject_command,
            subject_agent: r.subject_agent,
            model: r.model,
            outcome_bundle_id: r.outcome_bundle_id,
            reaction: r.reaction,
            delivery: r.delivery,
            outcome_source: r.outcome_source,
            counts_as_correction: r.counts_as_correction,
            live_verdict_id: r.live_verdict_id,
            outcome_projection_version: r.outcome_projection_version,
            legacy,
            pending,
        }
    }
}

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
