use super::*;
use crate::store::persistence::eval_repo::EvalPersist as _;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct SessionOverviewTurnDto {
    pub(crate) turn_id: i64,
    pub(crate) user_prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) stop_response: Option<String>,
    pub(crate) stop_reason: Option<String>,
    pub(crate) stop_request_id: Option<i64>,
    pub(crate) interrupted: bool,
    pub(crate) outcome: String,
    pub(crate) start_request_id: i64,
    pub(crate) end_request_id: i64,
    pub(crate) start_request_number: i64,
    pub(crate) end_request_number: i64,
    pub(crate) created_at: String,
    pub(crate) closed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) prompt_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) subject_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) subject_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) model: Option<String>,
    pub(crate) reaction: Option<String>,
    pub(crate) delivery: Option<String>,
    pub(crate) outcome_source: Option<String>,
    pub(crate) live_verdict_id: Option<i64>,
    pub(crate) outcome_projection_version: Option<i16>,
    pub(crate) legacy: bool,
    pub(crate) pending: bool,
}

impl From<crate::store::persistence::operator_turns::SessionOverviewTurnRow>
    for SessionOverviewTurnDto
{
    fn from(r: crate::store::persistence::operator_turns::SessionOverviewTurnRow) -> Self {
        let (legacy, pending) =
            crate::store::operator_turns::projection_flags(r.outcome_source.as_deref(), &r.outcome);
        Self {
            turn_id: r.turn_id,
            user_prompt: r.user_prompt,
            stop_response: r.stop_response,
            stop_reason: r.stop_reason,
            stop_request_id: r.stop_request_id,
            interrupted: r.interrupted,
            outcome: r.outcome,
            start_request_id: r.start_request_id,
            end_request_id: r.end_request_id,
            start_request_number: r.start_request_number,
            end_request_number: r.end_request_number,
            created_at: r.created_at.to_rfc3339(),
            closed_at: r.closed_at.map(|t| t.to_rfc3339()),
            prompt_kind: r.prompt_kind,
            subject_command: r.subject_command,
            subject_agent: r.subject_agent,
            model: r.model,
            reaction: r.reaction,
            delivery: r.delivery,
            outcome_source: r.outcome_source,
            live_verdict_id: r.live_verdict_id,
            outcome_projection_version: r.outcome_projection_version,
            legacy,
            pending,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct SessionOverviewDto {
    pub(crate) session_id: String,
    pub(crate) total_turns: i64,
    pub(crate) truncated: bool,
    pub(crate) offset: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) limit: Option<i64>,
    pub(crate) turns: Vec<SessionOverviewTurnDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) quality: Option<SessionQualityDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) narrative_overview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) narrative_overview_generated_at: Option<String>,
    pub(crate) embedding: EmbeddingStatusDto,
    pub(crate) knowledge: Option<SessionKnowledgeDto>,
    pub(crate) related_sessions: Vec<SessionOverviewRelatedSessionDto>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct SessionKnowledgeDto {
    pub(crate) topics: Vec<String>,
    pub(crate) learnings: Vec<String>,
    pub(crate) generated_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct SessionOverviewRelatedSessionDto {
    pub(crate) session_id: String,
    pub(crate) title: Option<String>,
    pub(crate) similarity: f64,
    pub(crate) outcome: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) overview_excerpt: Option<String>,
}

pub(crate) fn char_excerpt(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

async fn fetch_overview_turns(
    handle: &crate::store::persistence::PersistHandle,
    session_id: &str,
    db_id: i64,
    params: &SessionOperatorTurnsParams,
) -> Result<(Vec<SessionOverviewTurnDto>, i64, bool), Response> {
    let omit_stop_response = params
        .fields
        .as_deref()
        .map(|f| f.eq_ignore_ascii_case("prompts"))
        .unwrap_or(false);
    let filter = crate::store::persistence::operator_turns::SessionOverviewFilter {
        prompt_kind: params.prompt_kind.clone(),
        limit: params.limit,
        offset: params.offset,
        omit_stop_response,
    };
    let (rows, total_turns, truncated) = handle
        .session_overview_bounded(db_id, params.order.unwrap_or_default(), &filter)
        .await
        .map_err(|e| {
            tracing::warn!(
                session_id = %session_id,
                error = %e,
                "get_session_overview: query failed"
            );
            persist_error_response(e)
        })?;
    let turns = rows.into_iter().map(SessionOverviewTurnDto::from).collect();
    Ok((turns, total_turns, truncated))
}

async fn fetch_overview_quality(
    handle: &crate::store::persistence::PersistHandle,
    session_id: &str,
) -> Option<SessionQualityDto> {
    match handle.eval().fetch_session_quality(session_id).await {
        Ok(Some(q)) => Some(SessionQualityDto::from(q)),
        Ok(None) => None,
        Err(e) => {
            tracing::warn!(
                session_id = %session_id,
                error = %e,
                "get_session_overview: quality fetch failed — omitting from response"
            );
            None
        }
    }
}

async fn fetch_overview_narrative(
    handle: &crate::store::persistence::PersistHandle,
    session_id: &str,
    db_id: i64,
) -> (Option<String>, Option<String>) {
    match handle.fetch_session_overview_text(db_id).await {
        Ok(Some((text, generated_at))) => (Some(text), generated_at.map(|dt| dt.to_rfc3339())),
        Ok(None) => (None, None),
        Err(e) => {
            tracing::warn!(
                session_id = %session_id,
                error = %e,
                "get_session_overview: narrative fetch failed — omitting from response"
            );
            (None, None)
        }
    }
}

async fn fetch_overview_embedding(
    handle: &crate::store::persistence::PersistHandle,
    state: &AppState,
    session_id: &str,
    session_uuid: uuid::Uuid,
) -> EmbeddingStatusDto {
    match handle
        .knowledge()
        .session_embedding_status(session_uuid)
        .await
    {
        Ok(Some(s)) => EmbeddingStatusDto::Present {
            present: true,
            source: s.source,
            model: s.model,
            embedded_at: s.embedded_at.to_rfc3339(),
            chunk_count: s.chunk_count,
        },
        Ok(None) => {
            let reason = if state.embedding_service().is_available() {
                "no_embedded_turns"
            } else {
                "embedding_disabled"
            };
            EmbeddingStatusDto::Absent {
                present: false,
                reason,
            }
        }
        Err(e) => {
            tracing::warn!(session_id = %session_id, error = %e, "get_session_overview: embedding status fetch failed — reporting absent");
            EmbeddingStatusDto::Absent {
                present: false,
                reason: "no_embedded_turns",
            }
        }
    }
}

async fn fetch_overview_knowledge(
    handle: &crate::store::persistence::PersistHandle,
    session_id: &str,
    session_uuid: uuid::Uuid,
) -> Option<SessionKnowledgeDto> {
    match handle
        .knowledge()
        .fetch_session_knowledge(session_uuid)
        .await
    {
        Ok(Some(k)) => Some(SessionKnowledgeDto {
            topics: k.topics,
            learnings: k.learnings,
            generated_at: k.generated_at.to_rfc3339(),
        }),
        Ok(None) => None,
        Err(e) => {
            tracing::warn!(session_id = %session_id, error = %e, "get_session_overview: knowledge fetch failed — omitting");
            None
        }
    }
}

async fn resolve_neighbour_db_id(
    handle: &crate::store::persistence::PersistHandle,
    session_id: &str,
    neighbour_id: uuid::Uuid,
) -> Option<i64> {
    match handle
        .resolve_session_db_id(&neighbour_id.to_string())
        .await
    {
        Ok(Some(id)) => Some(id),
        Ok(None) => None,
        Err(e) => {
            tracing::warn!(session_id = %session_id, neighbour_id = %neighbour_id, error = %e, "get_session_overview: neighbour session resolve failed");
            None
        }
    }
}

async fn fetch_neighbour_overview_excerpt(
    handle: &crate::store::persistence::PersistHandle,
    session_id: &str,
    neighbour_id: uuid::Uuid,
) -> Option<String> {
    let neighbour_db_id = resolve_neighbour_db_id(handle, session_id, neighbour_id).await?;
    match handle.fetch_session_overview_text(neighbour_db_id).await {
        Ok(Some((text, _))) => Some(char_excerpt(&text, 300)),
        Ok(None) => None,
        Err(e) => {
            tracing::warn!(session_id = %session_id, neighbour_id = %neighbour_id, error = %e, "get_session_overview: neighbour overview fetch failed");
            None
        }
    }
}

async fn fetch_overview_related_sessions(
    handle: &crate::store::persistence::PersistHandle,
    session_id: &str,
    session_uuid: uuid::Uuid,
) -> Vec<SessionOverviewRelatedSessionDto> {
    let neighbours = match handle
        .knowledge()
        .nearest_sessions(session_uuid, 5, true, 0.0)
        .await
    {
        Ok(neighbours) => neighbours,
        Err(e) => {
            tracing::warn!(session_id = %session_id, error = %e, "get_session_overview: nearest_sessions failed — omitting");
            return Vec::new();
        }
    };
    let mut out = Vec::with_capacity(neighbours.len());
    for n in neighbours {
        let overview_excerpt =
            fetch_neighbour_overview_excerpt(handle, session_id, n.session_id).await;
        out.push(SessionOverviewRelatedSessionDto {
            session_id: n.session_id.to_string(),
            title: n.title,
            similarity: n.similarity,
            outcome: n.outcome,
            overview_excerpt,
        });
    }
    out
}

pub(crate) async fn get_session_overview(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Query(params): Query<SessionOperatorTurnsParams>,
) -> Result<Json<SessionOverviewDto>, Response> {
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
            tracing::warn!(session_id = %session_id, error = %e, "get_session_overview: session_db_id resolution failed");
            persist_error_response(e)
        })?;
    let Some(db_id) = session_db_id else {
        return Err((StatusCode::NOT_FOUND, "unknown session").into_response());
    };

    let (turns, total_turns, truncated) =
        fetch_overview_turns(handle, &session_id, db_id, &params).await?;
    let quality = fetch_overview_quality(handle, &session_id).await;
    let (narrative_overview, narrative_overview_generated_at) =
        fetch_overview_narrative(handle, &session_id, db_id).await;
    let embedding = fetch_overview_embedding(handle, &state, &session_id, session_uuid).await;
    let knowledge = fetch_overview_knowledge(handle, &session_id, session_uuid).await;
    let related_sessions = fetch_overview_related_sessions(handle, &session_id, session_uuid).await;

    Ok(Json(SessionOverviewDto {
        session_id,
        total_turns,
        truncated,
        offset: params.offset.unwrap_or(0),
        limit: params.limit,
        turns,
        quality,
        narrative_overview,
        narrative_overview_generated_at,
        embedding,
        knowledge,
        related_sessions,
    }))
}
