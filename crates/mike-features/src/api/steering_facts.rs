use crate::store::persistence::dossier_repo::DossierPersist as _;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::dossier::steering_facts::SteeringFacts;
use crate::state::AppState;
use crate::store::persistence::steering_facts::PersistedSteeringFacts;

#[derive(Debug, Deserialize)]
pub struct SteeringFactsQuery {
    pub as_of: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SteeringFactsView {
    pub session_uuid: String,
    pub turn_id: i64,
    pub as_of: DateTime<Utc>,
    pub computed: SteeringFacts,
    pub persisted: Option<PersistedSteeringFacts>,
}

fn error(status: StatusCode, message: String) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

fn persistence_error(e: &crate::store::persistence::PersistError) -> Response {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("persistence error: {e}"),
    )
}

fn parse_as_of(raw: Option<&str>) -> Result<Option<DateTime<Utc>>, Box<Response>> {
    raw.map(|value| {
        DateTime::parse_from_rfc3339(value)
            .map(|t| t.with_timezone(&Utc))
            .map_err(|e| {
                Box::new(error(
                    StatusCode::BAD_REQUEST,
                    format!("as_of {value:?} is not an RFC 3339 timestamp: {e}"),
                ))
            })
    })
    .transpose()
}

pub async fn get_turn_steering_facts(
    State(state): State<AppState>,
    Path((session_uuid, turn_id)): Path<(String, i64)>,
    Query(query): Query<SteeringFactsQuery>,
) -> Response {
    let requested = match parse_as_of(query.as_of.as_deref()) {
        Ok(t) => t,
        Err(resp) => return *resp,
    };
    let Some(persist) = state.persist() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; steering facts are not available".to_owned(),
        );
    };
    let turn = match persist.locate_turn(turn_id).await {
        Ok(Some(turn)) if turn.session_uuid == session_uuid => turn,
        Ok(_) => {
            return error(
                StatusCode::NOT_FOUND,
                format!("operator turn {turn_id} not found in session {session_uuid}"),
            );
        }
        Err(e) => return persistence_error(&e),
    };
    let as_of = requested.or(turn.closed_at).unwrap_or_else(Utc::now);
    let computed = match SteeringFacts::as_of(persist, turn.session_db_id, turn_id, as_of).await {
        Ok(facts) => facts,
        Err(e) => return persistence_error(&e),
    };
    let persisted = if turn.closed_at.is_some() {
        match persist.dossier().persisted_steering_facts(turn_id).await {
            Ok(row) => row,
            Err(e) => return persistence_error(&e),
        }
    } else {
        None
    };
    Json(SteeringFactsView {
        session_uuid,
        turn_id,
        as_of,
        computed,
        persisted,
    })
    .into_response()
}
