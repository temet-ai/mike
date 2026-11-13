use super::*;

pub fn narrative_overview_llm_error(message: String) -> Response {
    tracing::warn!(error = %message, "narrative overview generation failed");
    (StatusCode::BAD_GATEWAY, message).into_response()
}

pub async fn generate_session_narrative_overview(
    State(state): State<crate::state::AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<NarrativeOverviewResponse>, Response> {
    let Ok(session_uuid) = uuid::Uuid::parse_str(&session_id) else {
        return Err((StatusCode::NOT_FOUND, "unknown session").into_response());
    };
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let session_db_id = handle
        .resolve_session_db_id(&session_id)
        .await
        .map_err(persist_error_response)?;
    let Some(db_id) = session_db_id else {
        return Err((StatusCode::NOT_FOUND, "unknown session").into_response());
    };

    let digest = build_narrative_overview_digest(handle, db_id, &session_id)
        .await
        .map_err(persist_error_response)?;

    let timeout = std::time::Duration::from_secs(state.config.steerer.request_timeout_secs);
    let SteererClient { llm, .. } =
        steerer_client(&state, timeout).map_err(narrative_overview_llm_error)?;

    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "overview": {
                "type": "string",
                "description": "Exactly two paragraphs of prose: what the session set out to do, then where it landed."
            }
        },
        "required": ["overview"],
        "additionalProperties": false
    });

    let output = llm
        .verdict::<NarrativeOverviewLlmOutput>(NARRATIVE_OVERVIEW_SYSTEM_PROMPT, &digest, schema)
        .await
        .map_err(|e| narrative_overview_llm_error(format!("LLM call failed: {e}")))?;

    let overview = output.overview.trim().to_string();
    let generated_at = chrono::Utc::now();
    let updated = handle
        .set_session_overview(session_uuid, &overview, Some(generated_at))
        .await
        .map_err(persist_error_response)?;
    if !updated {
        return Err((StatusCode::NOT_FOUND, "unknown session").into_response());
    }

    Ok(Json(NarrativeOverviewResponse {
        session_id: session_uuid,
        overview,
        generated_at: Some(generated_at.to_rfc3339()),
    }))
}
