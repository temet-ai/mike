use super::*;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::steerer_repo::SteererPersist as _;

mod enrolment;
pub(crate) use enrolment::*;

pub(crate) fn steerer_to_dto(steerer: &crate::steerer::registry::Steerer) -> SteererDto {
    SteererDto::from(&crate::store::persistence::steering::SteererRow {
        id: steerer.id,
        uuid: steerer.uuid,
        name: steerer.name.clone(),
        display_name: steerer.display_name.clone(),
        description: steerer.description.clone(),
        agent_id: steerer.agent_id,
        authority: steerer.authority.clone(),
        mode: steerer.mode.clone(),
        model: steerer.model.clone(),
        priority: steerer.priority,
        enabled: steerer.enabled,
        config: steerer.config.clone(),
        prompt: steerer.prompt.clone(),
        kind: steerer.kind.clone(),
        trigger_point: steerer.trigger_point.clone(),
        invocation: steerer.invocation.clone(),
        pattern: steerer.pattern.clone(),
        auto_enrol: steerer.auto_enrol,
        project_id: steerer.project_id,
        project_path: steerer.project_path.clone(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    })
}

pub(crate) async fn patch_steerer(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Json(body): Json<SteererPatch>,
) -> Result<Json<SteererEnvelope>, (StatusCode, Json<SteererErrorBody>)> {
    let Some(svc) = state.steerer_service() else {
        return Err(steerer_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; steerer mutation is not available",
        ));
    };
    let Some(handle) = state.persist() else {
        return Err(steerer_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; steerer mutation is not available",
        ));
    };
    let registry = state.steerer_registry.load_full();
    let current = registry
        .steerers
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| steerer_error_response(StatusCode::NOT_FOUND, "no steerer with that id"))?;

    let display_name = body
        .display_name
        .as_deref()
        .unwrap_or(&current.display_name);
    let description = body.description.as_deref().unwrap_or(&current.description);
    let enabled = body.enabled.unwrap_or(current.enabled);
    let authority = body.authority.as_deref().unwrap_or(&current.authority);
    let mode = body.mode.as_deref().unwrap_or(&current.mode);
    let model = body.model.as_deref().unwrap_or(&current.model);
    if body.model.is_some() {
        check_steerer_model_shape(model)?;
    }
    let priority = body.priority.unwrap_or(current.priority);
    let config = body.config.as_ref().unwrap_or(&current.config);
    let prompt: Option<&str> = body.prompt.as_deref().or(current.prompt.as_deref());
    let trigger_point = body
        .trigger_point
        .as_deref()
        .unwrap_or(&current.trigger_point);
    let invocation = body.invocation.as_deref().unwrap_or(&current.invocation);
    let pattern: Option<&str> = body.pattern.as_deref().or(current.pattern.as_deref());

    let project_id = match body.project_path {
        None => current.project_id,
        Some(None) => None,
        Some(Some(ref path)) => resolve_steerer_project_id(handle, Some(path.as_str())).await?,
    };

    match svc
        .update(
            id,
            crate::steerer::service::SteererFields {
                display_name,
                description,
                enabled,
                authority_code: authority,
                mode_code: mode,
                model,
                priority,
                config,
                prompt,
                trigger_point,
                invocation,
                pattern,
                project_id,
            },
            &crate::web_base::change_source::change_source_from_headers(&headers),
        )
        .await
    {
        Ok(envelope) => Ok(Json(SteererEnvelope {
            steerer: steerer_to_dto(&envelope.steerer),
            warnings: envelope.warnings,
        })),
        Err(e) => Err(map_steerer_service_error(e)),
    }
}

pub(crate) async fn set_all_steerer_models(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SetAllSteererModelsBody>,
) -> Result<Json<SetAllSteererModelsResponse>, (StatusCode, Json<SteererErrorBody>)> {
    let Some(svc) = state.steerer_service() else {
        return Err(steerer_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; steerer mutation is not available",
        ));
    };

    check_steerer_model_listed(&state, &body.model).await?;

    let source = crate::web_base::change_source::change_source_from_headers(&headers);
    let registry = state.steerer_registry.load_full();
    let mut count: i64 = 0;
    for current in registry.steerers.iter() {
        svc.update(
            current.id,
            crate::steerer::service::SteererFields {
                display_name: &current.display_name,
                description: &current.description,
                enabled: current.enabled,
                authority_code: &current.authority,
                mode_code: &current.mode,
                model: &body.model,
                priority: current.priority,
                config: &current.config,
                prompt: current.prompt.as_deref(),
                trigger_point: &current.trigger_point,
                invocation: &current.invocation,
                pattern: current.pattern.as_deref(),
                project_id: current.project_id,
            },
            &source,
        )
        .await
        .map_err(map_steerer_service_error)?;
        count += 1;
    }

    Ok(Json(SetAllSteererModelsResponse { count }))
}

pub(crate) async fn delete_steerer(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> Result<Response, (StatusCode, Json<SteererErrorBody>)> {
    let Some(svc) = state.steerer_service() else {
        return Err(steerer_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; steerer deletion is not available",
        ));
    };
    let source = crate::web_base::change_source::change_source_from_headers(&headers);
    match svc.delete(id, &source).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT.into_response()),
        Err(e) => Err(map_steerer_service_error(e)),
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct SetSessionGoalBody {
    #[serde(default)]
    pub(crate) goal: Option<String>,
    #[serde(default)]
    pub(crate) next_steps: Option<String>,
    #[serde(default = "default_goal_set_by")]
    pub(crate) set_by: String,
    #[serde(default)]
    pub(crate) role: Option<String>,
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) instructions: Option<String>,
}

pub(crate) fn default_goal_set_by() -> String {
    "mcp".to_string()
}

#[derive(Debug, Serialize)]
pub(crate) struct SessionGoalDto {
    pub(crate) goal: Option<String>,
    pub(crate) next_steps: Option<String>,
    pub(crate) role: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) instructions: Option<String>,
}

pub(crate) async fn put_session_goal(
    State(state): State<AppState>,
    Path(session_uuid): Path<uuid::Uuid>,
    Json(body): Json<SetSessionGoalBody>,
) -> Response {
    let Some(persist) = state.persist() else {
        return persist_unavailable_response();
    };
    match persist
        .launch()
        .set_session_goal(
            crate::store::persistence::session_goals::SetSessionIdentity {
                session_uuid,
                display_name: body.name.as_deref(),
                role: body.role.as_deref(),
                goal: body.goal.as_deref(),
                next_steps: body.next_steps.as_deref(),
                instructions: body.instructions.as_deref(),
                set_by: &body.set_by,
            },
        )
        .await
    {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(crate::store::persistence::PersistError::DataInvariant(_)) => {
            (StatusCode::NOT_FOUND, "unknown session").into_response()
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                session_id = %session_uuid,
                "put_session_goal: DB write failed"
            );
            persist_error_response(e)
        }
    }
}

pub(crate) async fn get_session_goal(
    State(state): State<AppState>,
    Path(session_uuid): Path<uuid::Uuid>,
) -> Result<Json<SessionGoalDto>, StatusCode> {
    let Some(persist) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    match persist.launch().get_session_goal(session_uuid).await {
        Ok(sg) => Ok(Json(SessionGoalDto {
            goal: sg.as_ref().map(|g| g.goal.clone()),
            next_steps: sg.as_ref().map(|g| g.next_steps.clone()),
            role: sg.as_ref().and_then(|g| g.role.clone()),
            name: sg.as_ref().and_then(|g| g.display_name.clone()),
            instructions: sg.and_then(|g| g.instructions),
        })),
        Err(e) => {
            tracing::warn!(
                error = %e,
                session_id = %session_uuid,
                "get_session_goal: DB query failed"
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub(crate) async fn revert_intervention(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<RevertInterventionBody>,
) -> Result<Response, (StatusCode, Json<SteererErrorBody>)> {
    let Some(handle) = state.persist() else {
        return Err(steerer_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; intervention revert is not available",
        ));
    };
    match handle
        .steerer()
        .revert_intervention(id, &body.revert_reason)
        .await
    {
        Ok(true) => Ok(StatusCode::OK.into_response()),
        Ok(false) => Err(steerer_error_response(
            StatusCode::NOT_FOUND,
            "no intervention with that id, or already reverted",
        )),
        Err(e) => {
            tracing::warn!(error = %e, intervention_id = id, "revert_intervention: DB error");
            Err(steerer_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database error reverting intervention",
            ))
        }
    }
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct ModelsResponse {
    pub(crate) models: Vec<crate::steerer::model_catalogue::ModelEntry>,
}
