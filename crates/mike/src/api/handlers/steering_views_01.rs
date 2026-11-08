use super::*;

impl From<crate::store::persistence::steering::SteeredRequestRow> for SteeredRequestDto {
    fn from(r: crate::store::persistence::steering::SteeredRequestRow) -> Self {
        Self {
            request_id: r.request_id.unwrap_or(0),
            response_id: r.response_id,
            session_uuid: r.session_uuid.unwrap_or_default(),
            operator_turn_id: r.operator_turn_id,
            steerer_name: r.steerer_name.unwrap_or_default(),
            steerer_display_name: r.steerer_display_name.unwrap_or_default(),
            model: r.model,
            authority: r.authority.unwrap_or_default(),
            mode: r.mode.unwrap_or_default(),
            verdict: r.verdict.unwrap_or_default(),
            confidence: r.confidence,
            off_thread_topic: r.off_thread_topic,
            anchor_text: r.anchor_text,
            payload_text: r.payload_text,
            verdict_json: r.verdict_json,
            trigger_request_id: r.trigger_request_id,
            sent_at: r.sent_at.map(|t| t.to_rfc3339()),
            processed_at: r.processed_at.map(|t| t.to_rfc3339()),
            expires_at: r.expires_at.map(|t| t.to_rfc3339()),
            reverted_at: r.reverted_at.map(|t| t.to_rfc3339()),
            revert_reason: r.revert_reason,
            created_at: r.created_at.map(|t| t.to_rfc3339()).unwrap_or_default(),
        }
    }
}

pub(crate) async fn get_request_steering(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<SteeredRequestDto>, StatusCode> {
    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    match handle.fetch_steered_request(id).await {
        Ok(Some(row)) => Ok(Json(SteeredRequestDto::from(row))),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::warn!(error = %e, request_id = id, "get_request_steering: DB query failed");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Serialize)]
pub(crate) struct SessionSteeringRowDto {
    pub(crate) intervention_id: i64,
    pub(crate) session_id: i64,
    pub(crate) session_uuid: uuid::Uuid,
    pub(crate) project_path: String,
    pub(crate) steerer_name: String,
    pub(crate) steerer_display_name: String,
    pub(crate) verdict: String,
    pub(crate) authority: String,
    pub(crate) mode: String,
    pub(crate) trigger_request_id: Option<i64>,
    pub(crate) sent_request_id: Option<i64>,
    pub(crate) sent_response_id: Option<i64>,
    pub(crate) operator_turn_id: Option<i64>,
    pub(crate) gate_reason: Option<String>,
    pub(crate) off_thread_topic: Option<String>,
    pub(crate) confidence: Option<f64>,
    pub(crate) anchor_text: Option<String>,
    pub(crate) verdict_json: Option<serde_json::Value>,
    pub(crate) payload_text: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) sent_at: Option<String>,
    pub(crate) processed_at: Option<String>,
    pub(crate) expires_at: Option<String>,
    pub(crate) reverted_at: Option<String>,
    pub(crate) revert_reason: Option<String>,
    pub(crate) created_at: String,
}

impl From<crate::store::persistence::steering::SessionSteeringRow> for SessionSteeringRowDto {
    fn from(r: crate::store::persistence::steering::SessionSteeringRow) -> Self {
        Self {
            intervention_id: r.intervention_id.unwrap_or(0),
            session_id: r.session_id.unwrap_or(0),
            session_uuid: r.session_uuid.unwrap_or_default(),
            project_path: r.project_path.unwrap_or_default(),
            steerer_name: r.steerer_name.unwrap_or_default(),
            steerer_display_name: r.steerer_display_name.unwrap_or_default(),
            verdict: r.verdict.unwrap_or_default(),
            authority: r.authority.unwrap_or_default(),
            mode: r.mode.unwrap_or_default(),
            trigger_request_id: r.trigger_request_id,
            sent_request_id: r.sent_request_id,
            sent_response_id: r.sent_response_id,
            operator_turn_id: r.operator_turn_id,
            gate_reason: r.gate_reason,
            off_thread_topic: r.off_thread_topic,
            confidence: r.confidence,
            anchor_text: r.anchor_text,
            verdict_json: r.verdict_json,
            payload_text: r.payload_text,
            model: r.model,
            sent_at: r.sent_at.map(|t| t.to_rfc3339()),
            processed_at: r.processed_at.map(|t| t.to_rfc3339()),
            expires_at: r.expires_at.map(|t| t.to_rfc3339()),
            reverted_at: r.reverted_at.map(|t| t.to_rfc3339()),
            revert_reason: r.revert_reason,
            created_at: r.created_at.map(|t| t.to_rfc3339()).unwrap_or_default(),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct SessionSteeringParams {
    #[serde(default)]
    pub(crate) offset: Option<usize>,
    #[serde(default)]
    pub(crate) limit: Option<usize>,
}

pub(crate) const SESSION_STEERING_DEFAULT_LIMIT: usize = 50;
pub(crate) const SESSION_STEERING_MAX_LIMIT: usize = 500;

#[derive(Serialize)]
pub(crate) struct SessionSteeringResponse {
    pub(crate) rows: Vec<SessionSteeringRowDto>,
    pub(crate) total_count: i64,
    pub(crate) offset: usize,
    pub(crate) limit: usize,
}

async fn fetch_session_steering_count(
    handle: &crate::store::persistence::PersistHandle,
    session_uuid: uuid::Uuid,
) -> Result<i64, StatusCode> {
    handle
        .count_session_steering(session_uuid)
        .await
        .map_err(|e| {
            tracing::warn!(
                error = %e,
                session_id = %session_uuid,
                "get_session_steering: count query failed"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub(crate) async fn get_session_steering(
    State(state): State<AppState>,
    Path(session_uuid): Path<uuid::Uuid>,
    Query(params): Query<SessionSteeringParams>,
) -> Result<Json<SessionSteeringResponse>, StatusCode> {
    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let offset = params.offset.unwrap_or(0);
    let limit = params
        .limit
        .unwrap_or(SESSION_STEERING_DEFAULT_LIMIT)
        .clamp(1, SESSION_STEERING_MAX_LIMIT);

    let total_count = fetch_session_steering_count(handle, session_uuid).await?;

    let offset_i64 = i64::try_from(offset).unwrap_or(i64::MAX);
    let rows = handle
        .fetch_session_steering(session_uuid, offset_i64, limit as i64)
        .await
        .map_err(|e| {
            tracing::warn!(
                error = %e,
                session_id = %session_uuid,
                "get_session_steering: DB query failed"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(SessionSteeringResponse {
        rows: rows.into_iter().map(SessionSteeringRowDto::from).collect(),
        total_count,
        offset,
        limit,
    }))
}

#[derive(Serialize)]
pub(crate) struct SteeringVocabularyDto {
    pub(crate) authorities: Vec<VocabTermDto>,
    pub(crate) modes: Vec<VocabTermDto>,
    pub(crate) verdicts: Vec<VocabTermDto>,
}

#[derive(Serialize)]
pub(crate) struct VocabTermDto {
    pub(crate) code: String,
    pub(crate) description: String,
}

impl From<crate::store::persistence::steering::VocabTermRow> for VocabTermDto {
    fn from(r: crate::store::persistence::steering::VocabTermRow) -> Self {
        Self {
            code: r.code,
            description: r.description,
        }
    }
}

pub(crate) async fn get_steering_vocabulary(
    State(state): State<AppState>,
) -> Result<Json<SteeringVocabularyDto>, StatusCode> {
    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    match handle.fetch_steering_vocabulary().await {
        Ok(vocab) => Ok(Json(SteeringVocabularyDto {
            authorities: vocab
                .authorities
                .into_iter()
                .map(VocabTermDto::from)
                .collect(),
            modes: vocab.modes.into_iter().map(VocabTermDto::from).collect(),
            verdicts: vocab.verdicts.into_iter().map(VocabTermDto::from).collect(),
        })),
        Err(e) => {
            tracing::warn!(error = %e, "get_steering_vocabulary: DB query failed");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Serialize)]
pub(crate) struct SessionEnrolmentDto {
    pub(crate) enrolment_id: i64,
    pub(crate) session_id: i64,
    pub(crate) session_uuid: uuid::Uuid,
    pub(crate) steerer_id: i64,
    pub(crate) steerer_name: String,
    pub(crate) steerer_display_name: String,
    pub(crate) steerer_enabled: bool,
    pub(crate) mode: String,
    pub(crate) authority: String,
    pub(crate) enrolled_at: String,
    pub(crate) enrolled_by: String,
    pub(crate) disenrolled_at: Option<String>,
}

impl From<crate::store::persistence::steering::SessionEnrolmentRow> for SessionEnrolmentDto {
    fn from(r: crate::store::persistence::steering::SessionEnrolmentRow) -> Self {
        Self {
            enrolment_id: r.enrolment_id.unwrap_or(0),
            session_id: r.session_id.unwrap_or(0),
            session_uuid: r.session_uuid.unwrap_or_default(),
            steerer_id: r.steerer_id.unwrap_or(0),
            steerer_name: r.steerer_name.unwrap_or_default(),
            steerer_display_name: r.steerer_display_name.unwrap_or_default(),
            steerer_enabled: r.steerer_enabled.unwrap_or(false),
            mode: r.mode.unwrap_or_default(),
            authority: r.authority.unwrap_or_default(),
            enrolled_at: r.enrolled_at.map(|t| t.to_rfc3339()).unwrap_or_default(),
            enrolled_by: r.enrolled_by.unwrap_or_default(),
            disenrolled_at: r.disenrolled_at.map(|t| t.to_rfc3339()),
        }
    }
}

pub(crate) async fn get_session_enrolments(
    State(state): State<AppState>,
    Path(session_uuid): Path<uuid::Uuid>,
) -> Result<Json<Vec<SessionEnrolmentDto>>, StatusCode> {
    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    match handle.fetch_session_enrolments(session_uuid).await {
        Ok(rows) => Ok(Json(
            rows.into_iter().map(SessionEnrolmentDto::from).collect(),
        )),
        Err(e) => {
            tracing::warn!(
                error = %e,
                session_id = %session_uuid,
                "get_session_enrolments: DB query failed"
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Serialize)]
pub(crate) struct SessionVerifierCountsDto {
    pub(crate) fired: i64,
    pub(crate) delivered: i64,
}

pub(crate) async fn get_session_verifier_counts(
    State(state): State<AppState>,
    Path(session_uuid): Path<uuid::Uuid>,
) -> Result<Json<SessionVerifierCountsDto>, StatusCode> {
    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    match handle.fetch_verifier_counts_for_session(session_uuid).await {
        Ok((fired, delivered)) => Ok(Json(SessionVerifierCountsDto { fired, delivered })),
        Err(e) => {
            tracing::warn!(
                error = %e,
                session_id = %session_uuid,
                "get_session_verifier_counts: DB query failed"
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Serialize)]
pub(crate) struct SessionValidationStatusDto {
    pub(crate) uuid: uuid::Uuid,
    pub(crate) operator_turn_id: i64,
    pub(crate) status: String,
    pub(crate) intervention_uuid: Option<uuid::Uuid>,
    pub(crate) dispatched_at: String,
    pub(crate) resolved_at: Option<String>,
}

impl From<crate::store::persistence::steering_evaluations::EvaluationRow>
    for SessionValidationStatusDto
{
    fn from(r: crate::store::persistence::steering_evaluations::EvaluationRow) -> Self {
        Self {
            uuid: r.uuid,
            operator_turn_id: r.operator_turn_id,
            status: r.status,
            intervention_uuid: r.intervention_uuid,
            dispatched_at: r.dispatched_at.to_rfc3339(),
            resolved_at: r.resolved_at.map(|t| t.to_rfc3339()),
        }
    }
}

pub(crate) async fn get_session_validation_status(
    State(state): State<AppState>,
    Path(session_uuid): Path<uuid::Uuid>,
) -> Result<Json<SessionValidationStatusDto>, StatusCode> {
    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    match handle
        .get_latest_evaluation_for_session(session_uuid, "verifier")
        .await
    {
        Ok(Some(row)) => Ok(Json(SessionValidationStatusDto::from(row))),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::warn!(
                error = %e,
                session_id = %session_uuid,
                "get_session_validation_status: DB query failed"
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Serialize)]
pub(crate) struct SessionAnalyzerDto {
    pub(crate) analyzer_name: String,
    pub(crate) display_name: String,
    pub(crate) enabled: bool,
    pub(crate) hit_count: i64,
    pub(crate) is_intercept: bool,
}
