use super::*;

#[derive(Serialize)]
pub(crate) struct TeamDispatchDto {
    pub(crate) agent_label: String,
    pub(crate) description: String,
    pub(crate) first_seen: String,
    pub(crate) last_seen: String,
    pub(crate) request_count: i64,
}

impl From<crate::store::persistence::mailbox::TeamCommsDispatchRow> for TeamDispatchDto {
    fn from(r: crate::store::persistence::mailbox::TeamCommsDispatchRow) -> Self {
        Self {
            agent_label: r.agent_label,
            description: r.description,
            first_seen: r.first_seen.to_rfc3339(),
            last_seen: r.last_seen.to_rfc3339(),
            request_count: r.request_count,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct TeamMessageDto {
    pub(crate) ts: String,
    pub(crate) direction: String,
    pub(crate) counterpart: String,
    pub(crate) summary: Option<String>,
    pub(crate) body: String,
}

impl From<crate::store::persistence::mailbox::TeamCommsMessageRow> for TeamMessageDto {
    fn from(r: crate::store::persistence::mailbox::TeamCommsMessageRow) -> Self {
        Self {
            ts: r.ts.to_rfc3339(),
            direction: r.direction,
            counterpart: r.counterpart,
            summary: r.summary,
            body: r.body,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct TeamCommsResponse {
    pub(crate) dispatches: Vec<TeamDispatchDto>,
    pub(crate) messages: Vec<TeamMessageDto>,
}

async fn resolve_team_comms_session(
    handle: &crate::store::persistence::PersistHandle,
    session_uuid: uuid::Uuid,
) -> Result<i64, Response> {
    match handle.resolve_session_core(session_uuid).await {
        Ok(Some((id, _))) => Ok(id),
        Ok(None) => Err((StatusCode::NOT_FOUND, "unknown session").into_response()),
        Err(e) => {
            tracing::warn!(error = %e, %session_uuid, "get_session_team_comms: session lookup failed");
            Err(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to resolve session".to_string(),
            ))
        }
    }
}

async fn fetch_team_comms_rows(
    handle: &crate::store::persistence::PersistHandle,
    session_db_id: i64,
    session_uuid: uuid::Uuid,
) -> Result<
    (
        Vec<crate::store::persistence::mailbox::TeamCommsDispatchRow>,
        Vec<crate::store::persistence::mailbox::TeamCommsMessageRow>,
    ),
    Response,
> {
    let dispatches = handle
        .fetch_team_comms_dispatches(session_db_id)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, %session_uuid, "get_session_team_comms: dispatches fetch failed");
            mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to fetch team dispatches".to_string(),
            )
        })?;
    let messages = handle
        .fetch_team_comms_messages(session_db_id)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, %session_uuid, "get_session_team_comms: messages fetch failed");
            mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to fetch team messages".to_string(),
            )
        })?;
    Ok((dispatches, messages))
}

pub(crate) async fn get_session_team_comms(
    State(state): State<AppState>,
    Path(session_uuid): Path<uuid::Uuid>,
) -> Result<Json<TeamCommsResponse>, Response> {
    let Some(handle) = state.persist() else {
        return Err(mailbox_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence_disabled",
            "session persistence is disabled; team activity is not available".to_string(),
        ));
    };
    let session_db_id = resolve_team_comms_session(handle, session_uuid).await?;
    let (dispatches, messages) = fetch_team_comms_rows(handle, session_db_id, session_uuid).await?;

    Ok(Json(TeamCommsResponse {
        dispatches: dispatches.into_iter().map(TeamDispatchDto::from).collect(),
        messages: messages.into_iter().map(TeamMessageDto::from).collect(),
    }))
}
