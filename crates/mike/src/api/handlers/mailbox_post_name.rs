use super::*;
use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;

pub(crate) async fn resolve_name_addressed_recipient(
    handle: &crate::store::persistence::PersistHandle,
    name: &str,
    body: &PostSessionMessageBody,
) -> Result<uuid::Uuid, Response> {
    let name = name.trim();
    if name.is_empty() {
        return Err(mailbox_error(
            StatusCode::BAD_REQUEST,
            "invalid_name",
            "name: addressing requires a non-empty name after the prefix".to_string(),
        ));
    }
    let (from_session_uuid, sender_project_id) =
        resolve_name_sender_project(handle, body.from_session_id.as_deref())
            .await
            .map_err(|e| *e)?;

    match handle
        .launch()
        .resolve_name_recipient(sender_project_id, name, from_session_uuid)
        .await
    {
        Ok(crate::store::persistence::mailbox::SessionIdResolution::Resolved(uuid)) => Ok(uuid),
        Ok(crate::store::persistence::mailbox::SessionIdResolution::Ambiguous(candidates)) => {
            Err(ambiguous_session_name_response(name, candidates))
        }
        Ok(crate::store::persistence::mailbox::SessionIdResolution::NotFound) => {
            Err(mailbox_error(
                StatusCode::NOT_FOUND,
                "no_live_session_for_name",
                format!("no live session in this project declares name `{name}`"),
            ))
        }
        Err(e) => {
            tracing::warn!(error = %e, name, "resolve_name_addressed_recipient: name resolution failed");
            Err(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to resolve name recipient".to_string(),
            ))
        }
    }
}

async fn resolve_name_sender_project(
    handle: &crate::store::persistence::PersistHandle,
    from_raw: Option<&str>,
) -> Result<(uuid::Uuid, i64), Box<Response>> {
    let Some(from_raw) = from_raw else {
        return Err(Box::new(mailbox_error(
            StatusCode::BAD_REQUEST,
            "from_session_id_required",
            "name: addressing requires from_session_id to resolve the project scope to search within".to_string(),
        )));
    };
    let from_session_uuid = uuid::Uuid::parse_str(from_raw).map_err(|_| {
        Box::new(mailbox_error(
            StatusCode::BAD_REQUEST,
            "invalid_from_session_id",
            format!("from_session_id is not a valid UUID: {from_raw}"),
        ))
    })?;
    match handle
        .launch()
        .resolve_session_core(from_session_uuid)
        .await
    {
        Ok(Some((_, project_id))) => Ok((from_session_uuid, project_id)),
        Ok(None) => Err(Box::new(mailbox_error(
            StatusCode::NOT_FOUND,
            "from_session_not_found",
            format!("no session with uuid {from_session_uuid}"),
        ))),
        Err(e) => {
            tracing::warn!(error = %e, %from_session_uuid, "resolve_name_sender_project: sender lookup failed");
            Err(Box::new(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to resolve the sending session".to_string(),
            )))
        }
    }
}

pub(crate) fn is_fleet_form(to: &str) -> bool {
    let to = to.trim();
    to == "fleet" || to.starts_with("task:") || to.starts_with("owner:")
}

pub(crate) fn fleet_target_error_response(
    error: crate::store::persistence::mailbox::TargetError,
) -> Response {
    use crate::store::persistence::mailbox::TargetError;
    let (status, code) = match &error {
        TargetError::NotInFleet(_) => (StatusCode::BAD_REQUEST, "not_in_fleet"),
        TargetError::Empty(_) => (StatusCode::BAD_REQUEST, "invalid_fleet_target"),
        TargetError::UnknownTask { .. } => (StatusCode::NOT_FOUND, "unknown_task"),
        TargetError::NoSeatYet(_) => (StatusCode::NOT_FOUND, "no_seat_yet"),
        TargetError::NoOpenSeats => (StatusCode::NOT_FOUND, "no_open_seat"),
        TargetError::Persist(e) => {
            tracing::warn!(error = %e, "fleet mail target resolution failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "database_error")
        }
    };
    mailbox_error(status, code, error.to_string())
}

pub(crate) async fn resolve_fleet_addressed_recipient(
    handle: &crate::store::persistence::PersistHandle,
    to: &str,
    body: &PostSessionMessageBody,
) -> Result<uuid::Uuid, Response> {
    use crate::store::persistence::mailbox::FleetTarget;
    let Some(from_raw) = body.from_session_id.as_deref() else {
        return Err(mailbox_error(
            StatusCode::BAD_REQUEST,
            "from_session_id_required",
            format!(
                "`{}` addresses a seat of the sender's fleet and needs from_session_id to find it",
                to.trim()
            ),
        ));
    };
    let from = uuid::Uuid::parse_str(from_raw).map_err(|_| {
        mailbox_error(
            StatusCode::BAD_REQUEST,
            "invalid_from_session_id",
            format!("from_session_id is not a valid UUID: {from_raw}"),
        )
    })?;
    match handle.launch().resolve_fleet_target(from, to).await {
        Ok(Some(FleetTarget::Seat { session, .. })) => Ok(session),
        Ok(Some(FleetTarget::Seats(_)) | None) => Err(mailbox_error(
            StatusCode::BAD_REQUEST,
            "invalid_fleet_target",
            format!("`{}` does not name one seat", to.trim()),
        )),
        Err(e) => Err(fleet_target_error_response(e)),
    }
}

pub(crate) async fn tail_for_sender(
    state: &AppState,
    handle: &crate::store::persistence::PersistHandle,
    from: Option<uuid::Uuid>,
) -> Option<Vec<String>> {
    if !state.feature_toggle_overlay.load().change_record_tail {
        return None;
    }
    let from = from?;
    if !crate::launch::seat_fact::is_fleet_seat(handle, from).await {
        return None;
    }
    let row = handle.fleet().find_fleet_for_session(from).await.ok()??;
    let run = row.run_id.as_deref()?;
    let definition = crate::launch::fleet::FleetDefinition::from_value(&row.definition).ok()?;
    crate::api::changes::since_you_last_looked(state, &definition, run, from).await
}

fn ambiguous_session_name_response(name: &str, candidates: Vec<uuid::Uuid>) -> Response {
    let mut candidates: Vec<String> = candidates.iter().map(|u| u.to_string()).collect();
    candidates.sort();
    mailbox_error(
        StatusCode::CONFLICT,
        "ambiguous_session_name",
        format!(
            "name `{name}` matches {} live sessions: {} — address by session id/prefix or role: instead",
            candidates.len(),
            candidates.join(", ")
        ),
    )
}
