use super::*;
use crate::base_state::BaseState;
use crate::store::persistence::launch_repo::LaunchPersist as _;

#[derive(Serialize)]
pub struct TeamDispatchDto {
    pub agent_label: String,
    pub description: String,
    pub first_seen: String,
    pub last_seen: String,
    pub request_count: i64,
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
pub struct TeamMessageDto {
    pub ts: String,
    pub direction: String,
    pub counterpart: String,
    pub summary: Option<String>,
    pub body: String,
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
pub struct TeamCommsResponse {
    pub dispatches: Vec<TeamDispatchDto>,
    pub messages: Vec<TeamMessageDto>,
}

async fn resolve_team_comms_session(
    handle: &crate::store::persistence::PersistHandle,
    session_uuid: uuid::Uuid,
) -> Result<i64, Response> {
    match handle.launch().resolve_session_core(session_uuid).await {
        Ok(Some((id, _))) => Ok(id),
        Ok(None) => Err((StatusCode::NOT_FOUND, "unknown session").into_response()),
        Err(e) => {
            tracing::warn!(error = %e, session_id = %session_uuid, "get_session_team_comms: session lookup failed");
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
        .launch().fetch_team_comms_dispatches(session_db_id)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, session_id = %session_uuid, "get_session_team_comms: dispatches fetch failed");
            mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to fetch team dispatches".to_string(),
            )
        })?;
    let messages = handle
        .launch().fetch_team_comms_messages(session_db_id)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, session_id = %session_uuid, "get_session_team_comms: messages fetch failed");
            mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to fetch team messages".to_string(),
            )
        })?;
    Ok((dispatches, messages))
}

pub async fn get_session_team_comms(
    State(state): State<BaseState>,
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
