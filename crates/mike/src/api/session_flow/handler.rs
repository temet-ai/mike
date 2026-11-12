use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use uuid::Uuid;

use super::dto::{SessionFlowComponent, SessionFlowCursor, SessionFlowPage};
use super::map;
use crate::state::AppState;
use crate::store::persistence::session_flow::{FlowCursor, FlowReadError};

const DEFAULT_LIMIT: u16 = 50;

#[derive(Debug, Deserialize)]
pub(crate) struct SessionFlowQuery {
    component: SessionFlowComponent,
    cursor: Option<String>,
    limit: Option<u16>,
}

fn decode_cursor(
    raw: &str,
    session_id: &str,
    component: &SessionFlowComponent,
) -> Result<FlowCursor, StatusCode> {
    let wire: SessionFlowCursor = serde_json::from_str(raw).map_err(|_| StatusCode::BAD_REQUEST)?;
    if !wire.matches_scope(session_id, component) {
        return Err(StatusCode::BAD_REQUEST);
    }
    map::persist_cursor(&wire).ok_or(StatusCode::BAD_REQUEST)
}

pub(super) fn map_read_error(session_id: &str, error: FlowReadError) -> StatusCode {
    match error {
        FlowReadError::InvalidCursor | FlowReadError::UnsupportedComponent => {
            StatusCode::BAD_REQUEST
        }
        FlowReadError::Database(err) => {
            tracing::warn!(error = %err, session_id = %session_id, "get_session_flow: DB query failed");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

pub(crate) async fn get_session_flow(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<SessionFlowQuery>,
) -> Result<Json<SessionFlowPage>, StatusCode> {
    let Ok(session) = Uuid::parse_str(&id) else {
        return Err(StatusCode::NOT_FOUND);
    };
    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };

    let session_id = session.to_string();
    let cursor = query
        .cursor
        .as_deref()
        .map(|raw| decode_cursor(raw, &session_id, &query.component))
        .transpose()?;
    let component = map::persist_component(&query.component);
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, 500);

    let page = handle
        .fetch_session_flow(session, component, limit, cursor.as_ref())
        .await
        .map_err(|e| map_read_error(&session_id, e))?
        .ok_or(StatusCode::NOT_FOUND)?;

    let wire = map::wire_page(page).map_err(|e| {
        tracing::error!(error = %e, session_id = %session_id, "get_session_flow: cursor serialization failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(wire))
}
