use crate::store::persistence::eval_repo::EvalPersist as _;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use crate::state::AppState;
use crate::store::persistence::PersistError;
use crate::store::persistence::session_end::{EndLabelRow, SessionVerdictFull};

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 500;
const MODES: [&str; 3] = ["live", "shadow", "eval"];

#[derive(Debug, Default, Deserialize)]
pub struct SessionVerdictsQuery {
    pub mode: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct SessionVerdictsView {
    pub session_id: String,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
    pub end: Option<EndLabelRow>,
    pub verdicts: Vec<SessionVerdictFull>,
}

fn error(status: StatusCode, message: String) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

fn persistence_error(e: &PersistError) -> Response {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("persistence error: {e}"),
    )
}

pub fn validate_mode(mode: Option<&str>) -> Result<Option<&str>, String> {
    match mode {
        None => Ok(None),
        Some(m) if MODES.contains(&m) => Ok(Some(m)),
        Some(m) => Err(format!(
            "mode {m:?} is not one of live|shadow|eval; omit it for every mode"
        )),
    }
}

pub async fn get_session_verdicts(
    State(state): State<AppState>,
    Path(session_uuid): Path<String>,
    Query(query): Query<SessionVerdictsQuery>,
) -> Response {
    let mode = match validate_mode(query.mode.as_deref()) {
        Ok(m) => m,
        Err(message) => return error(StatusCode::BAD_REQUEST, message),
    };
    let Some(persist) = state.persist() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled".to_owned(),
        );
    };
    let session_db_id = match persist.resolve_session_db_id(&session_uuid).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return error(
                StatusCode::NOT_FOUND,
                format!("session {session_uuid} not found"),
            );
        }
        Err(e) => return persistence_error(&e),
    };
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = query.offset.unwrap_or(0).max(0);
    let (verdicts, total) = match persist
        .eval()
        .session_verdict_history(session_db_id, mode, limit, offset)
        .await
    {
        Ok(page) => page,
        Err(e) => return persistence_error(&e),
    };
    let end = match persist.eval().session_end_for_session(session_db_id).await {
        Ok(end) => end,
        Err(e) => return persistence_error(&e),
    };
    Json(SessionVerdictsView {
        session_id: session_uuid,
        total,
        offset,
        limit,
        end,
        verdicts,
    })
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_mode_unknown_mode_names_the_input() {
        assert_eq!(validate_mode(None), Ok(None));
        assert_eq!(validate_mode(Some("shadow")), Ok(Some("shadow")));
        let err = validate_mode(Some("draft")).err().unwrap_or_default();
        assert!(err.contains("\"draft\""));
    }
}
