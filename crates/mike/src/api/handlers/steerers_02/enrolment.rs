use super::*;

async fn fetch_steerer_project_scope(
    handle: &crate::store::persistence::PersistHandle,
    steerer_id: i64,
) -> Result<Option<i64>, (StatusCode, Json<SteererErrorBody>)> {
    match handle.fetch_steerer_by_id(steerer_id).await {
        Ok(Some(row)) => Ok(row.project_id),
        Ok(None) => Err(steerer_error_response(
            StatusCode::NOT_FOUND,
            "no steerer with that id",
        )),
        Err(e) => {
            tracing::warn!(error = %e, steerer_id, "enrol_session: steerer lookup failed");
            Err(steerer_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database error fetching steerer",
            ))
        }
    }
}

async fn check_session_matches_project(
    handle: &crate::store::persistence::PersistHandle,
    session_uuid: uuid::Uuid,
    steerer_project_id: i64,
) -> Result<(), (StatusCode, Json<SteererErrorBody>)> {
    match handle.get_session_project_id(session_uuid).await {
        Ok(Some(session_project_id)) => {
            if session_project_id != steerer_project_id {
                return Err(steerer_error_response(
                    StatusCode::CONFLICT,
                    "steerer is scoped to a different project than this session",
                ));
            }
            Ok(())
        }
        Ok(None) => Err(steerer_error_response(
            StatusCode::NOT_FOUND,
            "unknown session",
        )),
        Err(e) => {
            tracing::warn!(error = %e, session_uuid = %session_uuid, "enrol_session: session project lookup failed");
            Err(steerer_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database error resolving session project",
            ))
        }
    }
}

async fn validate_enrol_project_scope(
    handle: &crate::store::persistence::PersistHandle,
    session_uuid: uuid::Uuid,
    steerer_id: i64,
) -> Result<(), (StatusCode, Json<SteererErrorBody>)> {
    let Some(steerer_project_id) = fetch_steerer_project_scope(handle, steerer_id).await? else {
        return Ok(());
    };
    check_session_matches_project(handle, session_uuid, steerer_project_id).await
}

pub(crate) async fn enrol_session(
    State(state): State<AppState>,
    Path(session_uuid): Path<uuid::Uuid>,
    Json(body): Json<EnrolSessionBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<SteererErrorBody>)> {
    let Some(svc) = state.steerer_service() else {
        return Err(steerer_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; enrolment is not available",
        ));
    };
    let Some(handle) = state.persist() else {
        return Err(steerer_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; enrolment is not available",
        ));
    };

    validate_enrol_project_scope(handle, session_uuid, body.steerer_id).await?;

    match svc
        .enrol_session(
            session_uuid,
            body.steerer_id,
            &body.mode,
            &body.authority,
            &body.enrolled_by,
        )
        .await
    {
        Ok(enrolment_id) => Ok((
            StatusCode::CREATED,
            Json(serde_json::json!({ "enrolment_id": enrolment_id })),
        )),
        Err(e) => Err(map_steerer_service_error(e)),
    }
}

pub(crate) async fn disenrol_session(
    State(state): State<AppState>,
    Path((_session_uuid, enrolment_id)): Path<(uuid::Uuid, i64)>,
) -> Result<Response, (StatusCode, Json<SteererErrorBody>)> {
    let Some(svc) = state.steerer_service() else {
        return Err(steerer_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; disenrolment is not available",
        ));
    };
    match svc.disenrol_session(enrolment_id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT.into_response()),
        Ok(false) => Err(steerer_error_response(
            StatusCode::NOT_FOUND,
            "no enrolment with that id",
        )),
        Err(e) => Err(map_steerer_service_error(e)),
    }
}
