use crate::store::persistence::steerer_repo::SteererPersist as _;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use crate::change_log::current_build;
use crate::cross_session::CrossSessionSnapshot;
use crate::cross_session::calibration_block::calibration_block;
use crate::state::AppState;
use crate::store::persistence::change_log::NewChange;
use crate::web_base::change_source::change_source_from_headers;

const MAX_LIMIT: i64 = 200;
const PASS_RUNS_SHOWN: i64 = 30;

fn error(status: StatusCode, message: String) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn unavailable() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "session persistence is disabled".to_owned(),
    )
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct AsOfQuery {
    pub as_of: Option<chrono::DateTime<chrono::Utc>>,
    pub project_id: Option<i64>,
}

pub(crate) async fn get_calibration(
    State(state): State<AppState>,
    Query(q): Query<AsOfQuery>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let snapshot = match q.as_of {
        Some(at) => CrossSessionSnapshot::load_as_of(persist, at).await,
        None => Ok((*crate::cross_session::current()).clone()),
    };
    match snapshot {
        Ok(s) => {
            let questions: Vec<(&str, f64)> = [
                "relapse",
                "unbacked_wait",
                "authorised_handback",
                "unobserved_done",
            ]
            .iter()
            .map(|q| (*q, 0.75))
            .collect();
            let mut preview = s.clone();
            preview.settings.input_mode = "live".to_owned();
            let at = s.as_of.unwrap_or_else(chrono::Utc::now);
            let block = calibration_block(&preview, &questions, q.project_id, at);
            Json(json!({"snapshot": s, "block_if_live": block, "describes": s.describes()}))
                .into_response()
        }
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("persistence error: {e}"),
        ),
    }
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct PageQuery {
    pub status: Option<String>,
    pub cursor: Option<i64>,
    pub limit: Option<i64>,
    pub steerer_id: Option<i64>,
}

pub(crate) async fn get_thresholds(
    State(state): State<AppState>,
    Query(q): Query<PageQuery>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let steerer_id = match q.steerer_id {
        Some(id) => id,
        None => match persist.steerer().guard_steerer().await {
            Ok(Some(guard)) => guard.id,
            _ => {
                return error(
                    StatusCode::NOT_FOUND,
                    "the turn-guard steerer row is not seeded".to_owned(),
                );
            }
        },
    };
    let limit = q.limit.unwrap_or(50).clamp(1, MAX_LIMIT);
    let history = persist
        .steerer()
        .threshold_history(steerer_id, q.cursor, limit)
        .await;
    let passes = persist
        .steerer()
        .recent_threshold_pass_runs(PASS_RUNS_SHOWN)
        .await;
    match (history, passes) {
        (Ok(rows), Ok(passes)) => {
            let next = (i64::try_from(rows.len()).unwrap_or(0) == limit)
                .then(|| rows.last().map(|r| r.id))
                .flatten();
            Json(json!({"thresholds": rows, "next_cursor": next, "passes": passes})).into_response()
        }
        (Err(e), _) | (_, Err(e)) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("persistence error: {e}"),
        ),
    }
}

pub(crate) async fn list_proposals(
    State(state): State<AppState>,
    Query(q): Query<PageQuery>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let limit = q.limit.unwrap_or(50).clamp(1, MAX_LIMIT);
    match persist
        .steerer()
        .list_directive_proposals(q.status.as_deref(), q.cursor, limit)
        .await
    {
        Ok(rows) => {
            let next = (i64::try_from(rows.len()).unwrap_or(0) == limit)
                .then(|| rows.last().map(|r| r.id))
                .flatten();
            Json(json!({"proposals": rows, "next_cursor": next})).into_response()
        }
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("persistence error: {e}"),
        ),
    }
}

pub(crate) async fn get_proposal(State(state): State<AppState>, Path(id): Path<i64>) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    match persist.steerer().fetch_proposal_detail(id).await {
        Ok(Some(detail)) => Json(detail).into_response(),
        Ok(None) => error(StatusCode::NOT_FOUND, format!("no directive proposal {id}")),
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("persistence error: {e}"),
        ),
    }
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct RejectBody {
    pub note: Option<String>,
}

pub(crate) async fn reject_proposal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<RejectBody>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    match persist
        .steerer()
        .reject_directive_proposal(id, body.note.as_deref())
        .await
    {
        Ok(true) => {}
        Ok(false) => {
            let status = persist
                .steerer()
                .fetch_proposal_detail(id)
                .await
                .ok()
                .flatten()
                .map(|d| d.proposal.status);
            return (
                StatusCode::CONFLICT,
                Json(json!({"error": "the proposal is not open", "status": status})),
            )
                .into_response();
        }
        Err(e) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("persistence error: {e}"),
            );
        }
    }
    let source = change_source_from_headers(&headers).to_setting_json(&current_build());
    let key = format!("proposal:{id}");
    let old = json!("open");
    let new = json!({"status": "rejected", "note": body.note});
    let change = persist
        .append_change(&NewChange {
            subject_kind: "directive_decision",
            subject_key: &key,
            op: "update",
            old_value: Some(&old),
            new_value: Some(&new),
            source_json: &source,
        })
        .await;
    if let Err(e) = change {
        tracing::warn!(proposal = id, error = %e, "directive proposal: change-log row failed");
    }
    Json(json!({"id": id, "status": "rejected"})).into_response()
}
