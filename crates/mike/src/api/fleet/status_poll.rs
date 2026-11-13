use crate::store::persistence::fleet_repo::FleetPersist as _;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use serde_json::json;
use uuid::Uuid;

use super::definition_view::{DefinitionError, FleetView};
use super::definitions::{DEFAULT_LIST_LIMIT, ListFleetsQuery, MAX_LIST_LIMIT, load, persistence};
use crate::api::poll_answers::{FLEET, Loaded, Poll, answer_json, json_bytes};
use crate::launch::fleet::status::assemble;
use crate::launch::seat_fact::is_fleet_seat;
use crate::state::AppState;

fn fleet_poll(key: String, headers: &HeaderMap) -> Poll<'_> {
    Poll {
        key,
        freshness: FLEET,
        scope: Vec::new(),
        headers,
    }
}

fn loaded(value: Bytes) -> Loaded<Bytes> {
    Loaded {
        value,
        sessions: Vec::new(),
    }
}

async fn status_by_key(state: &AppState, key: &str) -> Result<Loaded<Bytes>, Response> {
    let persist = persistence(state).map_err(IntoResponse::into_response)?;
    let row = load(persist, key)
        .await
        .map_err(IntoResponse::into_response)?;
    let status = assemble(state, &row)
        .await
        .map_err(|e| DefinitionError::Status(e).into_response())?;
    let status = super::observations::with_observations(persist, row.run_id.as_deref(), &status)
        .await
        .map_err(IntoResponse::into_response)?;
    Ok(loaded(json_bytes(&status)?))
}

async fn status_by_session(state: &AppState, session: Uuid) -> Result<Loaded<Bytes>, Response> {
    let persist = persistence(state).map_err(IntoResponse::into_response)?;
    if !is_fleet_seat(persist, session).await {
        return Err(DefinitionError::NotFound(session.to_string()).into_response());
    }
    let row = persist
        .fleet()
        .find_fleet_for_session(session)
        .await
        .map_err(|e| DefinitionError::from(e).into_response())?
        .ok_or_else(|| DefinitionError::NotFound(session.to_string()).into_response())?;
    let status = assemble(state, &row)
        .await
        .map_err(|e| DefinitionError::Status(e).into_response())?;
    let status = super::observations::with_observations(persist, row.run_id.as_deref(), &status)
        .await
        .map_err(IntoResponse::into_response)?;
    Ok(loaded(json_bytes(&status)?))
}

async fn listed(
    state: &AppState,
    project_path: Option<&str>,
    limit: i64,
) -> Result<Loaded<Bytes>, Response> {
    let rows = persistence(state)
        .map_err(IntoResponse::into_response)?
        .fleet()
        .list_fleet_headers(project_path, limit)
        .await
        .map_err(|e| DefinitionError::from(e).into_response())?;
    let fleets: Vec<FleetView> = rows.iter().map(FleetView::listed).collect();
    Ok(loaded(json_bytes(&json!({ "fleets": fleets }))?))
}

pub(crate) async fn get_fleet_status(
    State(state): State<AppState>,
    Path(key): Path<String>,
    headers: HeaderMap,
) -> Response {
    let poll = fleet_poll(format!("fleet-status:{key}"), &headers);
    answer_json(&state, poll, || status_by_key(&state, &key)).await
}

pub(crate) async fn get_fleet_status_for_session(
    State(state): State<AppState>,
    Path(session): Path<Uuid>,
    headers: HeaderMap,
) -> Response {
    let poll = fleet_poll(format!("fleet-status-session:{session}"), &headers);
    answer_json(&state, poll, || status_by_session(&state, session)).await
}

pub(crate) async fn list_fleets(
    State(state): State<AppState>,
    Query(query): Query<ListFleetsQuery>,
    headers: HeaderMap,
) -> Response {
    let limit = query
        .limit
        .unwrap_or(DEFAULT_LIST_LIMIT)
        .clamp(1, MAX_LIST_LIMIT);
    let poll = fleet_poll(format!("fleets:{:?}:{limit}", query.project_path), &headers);
    answer_json(&state, poll, || {
        listed(&state, query.project_path.as_deref(), limit)
    })
    .await
}
