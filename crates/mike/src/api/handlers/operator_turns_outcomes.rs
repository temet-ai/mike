use super::*;

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct OperatorTurnOutcomeSummary {
    pub(crate) counts: Vec<OperatorTurnOutcomeCount>,
    pub(crate) total: i64,
    pub(crate) corrections: i64,
    pub(crate) frustration_closes: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct OperatorTurnOutcomeCount {
    pub(crate) outcome: String,
    pub(crate) count: i64,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct OperatorTurnsSummaryParams {
    #[serde(default)]
    pub(crate) project: Option<String>,
    #[serde(default)]
    pub(crate) since: Option<String>,
}

pub(crate) async fn get_operator_turns_outcome_summary(
    State(state): State<AppState>,
    Query(params): Query<OperatorTurnsSummaryParams>,
) -> Result<Json<OperatorTurnOutcomeSummary>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let since = params.since.as_deref().and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
    });
    let rows = handle
        .operator_turn_outcome_summary(params.project.as_deref(), since)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "operator_turns_outcome_summary: query failed");
            persist_error_response(e)
        })?;
    Ok(Json(summarise_outcomes(rows)))
}

pub(crate) fn summarise_outcomes(rows: Vec<(String, bool, i64)>) -> OperatorTurnOutcomeSummary {
    let mut counts: Vec<OperatorTurnOutcomeCount> = Vec::new();
    let mut corrections = 0;
    let mut frustration_closes = 0;
    for (outcome, frustration_close, count) in rows {
        let source = frustration_close.then_some("frustration_close");
        if crate::store::persistence::operator_turns::correction::counts_as_correction(
            &outcome, source,
        ) {
            corrections += count;
        }
        if frustration_close {
            frustration_closes += count;
        }
        match counts.iter_mut().find(|c| c.outcome == outcome) {
            Some(existing) => existing.count += count,
            None => counts.push(OperatorTurnOutcomeCount { outcome, count }),
        }
    }
    let total = counts.iter().map(|c| c.count).sum();
    OperatorTurnOutcomeSummary {
        counts,
        total,
        corrections,
        frustration_closes,
    }
}

pub(crate) async fn list_operator_turns_by_outcome(
    State(state): State<AppState>,
    Query(params): Query<OperatorTurnsListParams>,
) -> Result<impl axum::response::IntoResponse, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let correction = params.outcome.as_deref() == Some("correction");
    let outcome = match params.outcome.as_deref() {
        Some("unknown" | "accepted" | "corrected" | "abandoned" | "shipped") => {
            params.outcome.clone()
        }
        Some("correction") => None,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "missing or invalid `outcome` query parameter; must be one of unknown|accepted|corrected|abandoned|shipped|correction",
                })),
            )
                .into_response());
        }
    };
    let limit = params
        .limit
        .unwrap_or(OPERATOR_TURNS_DEFAULT_LIMIT)
        .clamp(1, OPERATOR_TURNS_MAX_LIMIT) as i64;
    let offset = params.offset.unwrap_or(0) as i64;
    let date_from_str = params.date_from.as_deref().or(params.since.as_deref());
    let window =
        crate::api::search_window::resolve_window(date_from_str, params.date_to.as_deref(), false)
            .map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    axum::Json(serde_json::json!({"error": e.to_string()})),
                )
                    .into_response()
            })?;
    let session_db_id = if let Some(ref uuid) = params.session {
        handle.resolve_session_db_id(uuid).await.map_err(|e| {
            tracing::warn!(error = %e, "list_operator_turns_by_outcome: session_db_id resolution failed");
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
        tracing::warn!(error = %e, "list_operator_turns_by_outcome: query failed");
        persist_error_response(e)
    })?;
    let dtos: Vec<OperatorTurnDto> = rows.into_iter().map(OperatorTurnDto::from).collect();
    let header_map = window_note_headers(&window);
    Ok((header_map, axum::Json(dtos)).into_response())
}

#[cfg(test)]
mod summary_tests {
    use super::*;

    #[test]
    fn summarise_outcomes_frustration_close_counts_as_correction() {
        let summary = summarise_outcomes(vec![
            ("abandoned".to_owned(), false, 2),
            ("abandoned".to_owned(), true, 3),
            ("accepted".to_owned(), false, 5),
            ("corrected".to_owned(), false, 4),
        ]);
        assert_eq!(summary.total, 14);
        assert_eq!(summary.corrections, 7);
        assert_eq!(summary.frustration_closes, 3);
        let abandoned = summary.counts.iter().find(|c| c.outcome == "abandoned");
        assert_eq!(abandoned.map(|c| c.count), Some(5));
    }
}
