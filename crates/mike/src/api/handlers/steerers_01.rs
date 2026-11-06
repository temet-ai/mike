use super::*;

impl From<&crate::store::persistence::steering::SteererRow> for SteererDto {
    fn from(r: &crate::store::persistence::steering::SteererRow) -> Self {
        Self {
            id: r.id,
            uuid: r.uuid,
            name: r.name.clone(),
            display_name: r.display_name.clone(),
            description: r.description.clone(),
            agent_id: r.agent_id,
            authority: r.authority.clone(),
            mode: r.mode.clone(),
            model: r.model.clone(),
            effective_model: r.model.clone(),
            model_inherited: r.model.is_empty(),
            priority: r.priority,
            enabled: r.enabled,
            config: r.config.clone(),
            prompt: r.prompt.clone(),
            created_at: r.created_at.to_rfc3339(),
            updated_at: r.updated_at.to_rfc3339(),
            kind: r.kind.clone(),
            trigger_point: r.trigger_point.clone(),
            invocation: r.invocation.clone(),
            pattern: r.pattern.clone(),
            auto_enrol: r.auto_enrol,
            enrolment_required: !r.auto_enrol,
            project_id: r.project_id,
            project_path: r.project_path.clone(),
            active_enrolment_count: 0,
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct SteererEnvelope {
    pub(crate) steerer: SteererDto,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SteererCreate {
    pub(crate) name: String,
    pub(crate) display_name: String,
    #[serde(default)]
    pub(crate) description: String,
    #[serde(default)]
    pub(crate) agent_id: Option<i64>,
    pub(crate) authority: String,
    pub(crate) mode: String,
    pub(crate) model: String,
    #[serde(default)]
    pub(crate) priority: i32,
    #[serde(default = "default_enabled")]
    pub(crate) enabled: bool,
    #[serde(default = "serde_json::Value::default")]
    pub(crate) config: serde_json::Value,
    #[serde(default)]
    pub(crate) prompt: Option<String>,
    #[serde(default = "default_trigger_point")]
    pub(crate) trigger_point: String,
    #[serde(default = "default_invocation")]
    pub(crate) invocation: String,
    #[serde(default)]
    pub(crate) pattern: Option<String>,
    #[serde(default)]
    pub(crate) project_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SteererPatch {
    #[serde(default)]
    pub(crate) display_name: Option<String>,
    #[serde(default)]
    pub(crate) description: Option<String>,
    #[serde(default)]
    pub(crate) enabled: Option<bool>,
    #[serde(default)]
    pub(crate) authority: Option<String>,
    #[serde(default)]
    pub(crate) mode: Option<String>,
    #[serde(default)]
    pub(crate) model: Option<String>,
    #[serde(default)]
    pub(crate) priority: Option<i32>,
    #[serde(default)]
    pub(crate) config: Option<serde_json::Value>,
    #[serde(default)]
    pub(crate) prompt: Option<String>,
    #[serde(default)]
    pub(crate) trigger_point: Option<String>,
    #[serde(default)]
    pub(crate) invocation: Option<String>,
    #[serde(default)]
    pub(crate) pattern: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable_string")]
    pub(crate) project_path: Option<Option<String>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SetAllSteererModelsBody {
    pub(crate) model: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct SetAllSteererModelsResponse {
    pub(crate) count: i64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EnrolSessionBody {
    pub(crate) steerer_id: i64,
    pub(crate) mode: String,
    pub(crate) authority: String,
    #[serde(default = "default_enrolled_by")]
    pub(crate) enrolled_by: String,
}

pub(crate) fn default_enrolled_by() -> String {
    "operator".to_string()
}

#[derive(Debug, Deserialize)]
pub(crate) struct RevertInterventionBody {
    #[serde(default)]
    pub(crate) revert_reason: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct SteererErrorBody {
    pub(crate) error: String,
}

pub(crate) fn steerer_error_response(
    status: StatusCode,
    error: &str,
) -> (StatusCode, Json<SteererErrorBody>) {
    (
        status,
        Json(SteererErrorBody {
            error: error.to_string(),
        }),
    )
}

pub(crate) fn map_steerer_service_error(
    e: crate::steerer::service::SteeringServiceError,
) -> (StatusCode, Json<SteererErrorBody>) {
    use crate::steerer::service::SteeringServiceError;
    match e {
        SteeringServiceError::NotFound => {
            steerer_error_response(StatusCode::NOT_FOUND, "no steerer with that id")
        }
        SteeringServiceError::Persistence(persist_err) => {
            tracing::error!(error = %persist_err, "steerer persistence failure");
            steerer_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database error while processing steerer",
            )
        }
    }
}

pub(crate) async fn resolve_steerer_project_id(
    handle: &crate::store::persistence::PersistHandle,
    project_path: Option<&str>,
) -> Result<Option<i64>, (StatusCode, Json<SteererErrorBody>)> {
    let Some(path) = project_path else {
        return Ok(None);
    };
    match handle.get_project_ids_for_path(path).await {
        Ok(ids) if ids.is_empty() => Err(steerer_error_response(
            StatusCode::BAD_REQUEST,
            "unknown project_path",
        )),
        Ok(ids) => Ok(ids.into_iter().next()),
        Err(e) => {
            tracing::warn!(error = %e, project_path = path, "steerer: project_path resolution failed");
            Err(steerer_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database error resolving project_path",
            ))
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListSteerersParams {
    #[serde(default)]
    pub(crate) project: Option<String>,
}

pub(crate) async fn list_steerers(
    State(state): State<AppState>,
    Query(params): Query<ListSteerersParams>,
) -> Json<Vec<SteererDto>> {
    let registry = state.steerer_registry.load_full();

    let project_filter_ids: Vec<i64> = match params.project.as_deref() {
        Some(path) => match state.persist() {
            Some(handle) => handle
                .get_project_ids_for_path(path)
                .await
                .unwrap_or_default(),
            None => Vec::new(),
        },
        None => Vec::new(),
    };

    let active_enrolment_counts: std::collections::HashMap<i64, i64> = match state.persist() {
        Some(handle) => handle
            .count_active_enrolments_all()
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(
                    error = %e,
                    "list_steerers: active-enrolment count query failed; defaulting to zero"
                );
                std::collections::HashMap::new()
            }),
        None => std::collections::HashMap::new(),
    };

    let dtos: Vec<SteererDto> = registry
        .steerers
        .iter()
        .filter(|s| {
            project_filter_ids.is_empty()
                || s.project_id.is_none()
                || s.project_id
                    .is_some_and(|pid| project_filter_ids.contains(&pid))
        })
        .map(|s| SteererDto {
            id: s.id,
            uuid: s.uuid,
            name: s.name.clone(),
            display_name: s.display_name.clone(),
            description: s.description.clone(),
            agent_id: s.agent_id,
            authority: s.authority.clone(),
            mode: s.mode.clone(),
            model: s.model.clone(),
            model_inherited: s.model.is_empty(),
            effective_model: if s.model.is_empty() {
                state.config.steerer.model.clone()
            } else {
                s.model.clone()
            },
            priority: s.priority,
            enabled: s.enabled,
            config: s.config.clone(),
            prompt: s.prompt.clone(),
            created_at: s.created_at.to_rfc3339(),
            updated_at: s.updated_at.to_rfc3339(),
            kind: s.kind.clone(),
            trigger_point: s.trigger_point.clone(),
            invocation: s.invocation.clone(),
            pattern: s.pattern.clone(),
            auto_enrol: s.auto_enrol,
            enrolment_required: !s.auto_enrol,
            project_id: s.project_id,
            project_path: s.project_path.clone(),
            active_enrolment_count: active_enrolment_counts.get(&s.id).copied().unwrap_or(0),
        })
        .collect();
    Json(dtos)
}

pub(crate) async fn get_steerer(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<SteererDto>, (StatusCode, Json<SteererErrorBody>)> {
    let Some(handle) = state.persist() else {
        return Err(steerer_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; steerer lookup is not available",
        ));
    };
    match handle.fetch_steerer_by_id(id).await {
        Ok(Some(row)) => Ok(Json(SteererDto::from(&row))),
        Ok(None) => Err(steerer_error_response(
            StatusCode::NOT_FOUND,
            "no steerer with that id",
        )),
        Err(e) => {
            tracing::warn!(error = %e, steerer_id = id, "get_steerer: DB query failed");
            Err(steerer_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database error fetching steerer",
            ))
        }
    }
}

pub(crate) async fn create_steerer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SteererCreate>,
) -> Result<(StatusCode, Json<SteererEnvelope>), (StatusCode, Json<SteererErrorBody>)> {
    let Some(svc) = state.steerer_service() else {
        return Err(steerer_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; steerer creation is not available",
        ));
    };
    let Some(handle) = state.persist() else {
        return Err(steerer_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; steerer creation is not available",
        ));
    };
    check_steerer_model_shape(&body.model)?;
    let project_id = resolve_steerer_project_id(handle, body.project_path.as_deref()).await?;

    match svc
        .create(
            &body.name,
            body.agent_id,
            crate::steerer::service::SteererFields {
                display_name: &body.display_name,
                description: &body.description,
                enabled: body.enabled,
                authority_code: &body.authority,
                mode_code: &body.mode,
                model: &body.model,
                priority: body.priority,
                config: &body.config,
                prompt: body.prompt.as_deref(),
                trigger_point: &body.trigger_point,
                invocation: &body.invocation,
                pattern: body.pattern.as_deref(),
                project_id,
            },
            &crate::api::change_source::change_source_from_headers(&headers),
        )
        .await
    {
        Ok(envelope) => Ok((
            StatusCode::CREATED,
            Json(SteererEnvelope {
                steerer: steerer_to_dto(&envelope.steerer),
                warnings: envelope.warnings,
            }),
        )),
        Err(e) => Err(map_steerer_service_error(e)),
    }
}
