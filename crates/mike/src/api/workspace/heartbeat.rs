use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::proxy_repo::ProxyPersist as _;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::state::AppState;
use crate::store::persistence::PersistError;
use crate::web_base::{persist_error_response, persist_unavailable_response};

#[derive(Debug, Deserialize)]
pub(crate) struct HeartbeatRequestBody {
    pub(crate) session_id: Uuid,
}

#[derive(Debug, Serialize)]
pub(crate) struct InboxCountsDto {
    pub(crate) unconsumed: i64,
    pub(crate) unclaimed: i64,
    pub(crate) dead_lettered_outbox: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct HeartbeatResponseDto {
    pub(crate) lease_expires_at: String,
    pub(crate) inbox: InboxCountsDto,
}

pub(crate) async fn heartbeat(
    State(state): State<AppState>,
    Json(body): Json<HeartbeatRequestBody>,
) -> Result<Json<HeartbeatResponseDto>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };

    let renewed = match handle.renew_workspace_lease(body.session_id).await {
        Ok(r) => r,
        Err(PersistError::RegistrationNotFound(_)) => {
            return Err(super::workspace_error(
                StatusCode::NOT_FOUND,
                "registration_not_found",
                format!(
                    "no active workspace registration for session {}",
                    body.session_id
                ),
            ));
        }
        Err(e) => return Err(persist_error_response(e)),
    };

    let counts = handle
        .launch()
        .fetch_inbox_counts(renewed.session_id)
        .await
        .map_err(persist_error_response)?;

    Ok(Json(HeartbeatResponseDto {
        lease_expires_at: renewed.lease_expires_at.to_rfc3339(),
        inbox: InboxCountsDto {
            unconsumed: counts.unconsumed,
            unclaimed: counts.unclaimed,
            dead_lettered_outbox: counts.dead_lettered_outbox,
        },
    }))
}

#[derive(Debug, Deserialize)]
pub(crate) struct WakePendingQuery {
    pub(crate) pid: i32,
}

#[derive(Debug, Serialize)]
pub(crate) struct WakePendingResponseDto {
    pub(crate) session_id: Uuid,
    pub(crate) pending_steering: usize,
    pub(crate) pending_total: usize,
    pub(crate) busy: bool,
}

pub(crate) async fn get_wake_pending(
    State(state): State<AppState>,
    Query(query): Query<WakePendingQuery>,
) -> Result<Json<WakePendingResponseDto>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };

    let session_uuid = handle
        .proxy()
        .find_live_session_by_pid(query.pid)
        .await
        .map_err(persist_error_response)?;

    let Some(session_uuid) = session_uuid else {
        return Err(super::workspace_error(
            StatusCode::NOT_FOUND,
            "registration_not_found",
            format!("no live workspace registration for pid {}", query.pid),
        ));
    };

    let session_id_str = session_uuid.to_string();
    let reminders = state.store.read().await.peek_reminders(&session_id_str);
    let pending_steering = reminders
        .iter()
        .filter(|r| {
            matches!(
                r.source,
                crate::store::models::ReminderSource::Steerer { .. }
            )
        })
        .count();
    let pending_total = reminders.len();
    let busy = state.in_flight.is_session_busy(&session_id_str);

    Ok(Json(WakePendingResponseDto {
        session_id: session_uuid,
        pending_steering,
        pending_total,
        busy,
    }))
}
