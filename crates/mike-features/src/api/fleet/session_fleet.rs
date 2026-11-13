use crate::store::persistence::fleet_repo::FleetPersist as _;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::launch::seat_fact::is_fleet_seat;
use crate::state::AppState;
use crate::web_base::{persist_error_response, persist_unavailable_response};

const FLEET_MEMBERSHIP_MIN_WINDOW_HOURS: i32 = super::DEFAULT_WINDOW_HOURS;

#[derive(Serialize)]
pub struct SessionFleetDto {
    pub session_id: String,
    pub project_path: Option<String>,
    pub in_fleet: bool,
    pub peer_count: i64,
    pub message_count: i64,
    pub fleet_size: i64,
    pub window_hours: i32,
}

fn compute_fleet_window_hours(fleet_size: i64, oldest_last_seen: Option<DateTime<Utc>>) -> i32 {
    if fleet_size <= 1 {
        return FLEET_MEMBERSHIP_MIN_WINDOW_HOURS;
    }
    let Some(oldest) = oldest_last_seen else {
        return FLEET_MEMBERSHIP_MIN_WINDOW_HOURS;
    };
    let hours_since = ((Utc::now() - oldest).num_seconds().max(0) as f64) / 3600.0;
    let computed = hours_since.ceil() as i32 + 1;
    computed.max(FLEET_MEMBERSHIP_MIN_WINDOW_HOURS)
}

pub async fn get_session_fleet(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionFleetDto>, Response> {
    let Ok(parsed_uuid) = Uuid::parse_str(&session_id) else {
        return Err((StatusCode::NOT_FOUND, "unknown session").into_response());
    };
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };

    let session_db_id = match handle.resolve_session_db_id(&session_id).await {
        Ok(Some(id)) => id,
        Ok(None) => return Err((StatusCode::NOT_FOUND, "unknown session").into_response()),
        Err(e) => return Err(persist_error_response(e)),
    };

    let row = handle
        .fleet()
        .fetch_session_fleet_membership(session_db_id)
        .await
        .map_err(persist_error_response)?;

    let window_hours = compute_fleet_window_hours(row.fleet_size, row.oldest_last_seen);

    Ok(Json(SessionFleetDto {
        session_id: parsed_uuid.to_string(),
        project_path: row.project_path,
        in_fleet: is_fleet_seat(handle, parsed_uuid).await,
        peer_count: row.peer_count,
        message_count: row.message_count,
        fleet_size: row.fleet_size,
        window_hours,
    }))
}
