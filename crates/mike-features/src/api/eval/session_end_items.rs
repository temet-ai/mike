use crate::store::persistence::eval_repo::EvalPersist as _;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use crate::state::AppState;
use crate::store::persistence::session_end::SessionEndItemRow;

const DEFAULT_LIMIT: i64 = 100;
const MAX_LIMIT: i64 = 1000;

#[derive(Debug, Default, Deserialize)]
pub struct SessionEndItemsQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct SessionEndItemsView {
    pub run_id: i64,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
    pub items: Vec<SessionEndItemRow>,
}

fn error(status: StatusCode, message: String) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

pub async fn list_session_end_items(
    State(state): State<AppState>,
    Path(run_id): Path<i64>,
    Query(query): Query<SessionEndItemsQuery>,
) -> Response {
    let Some(persist) = state.persist() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled".to_owned(),
        );
    };
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = query.offset.unwrap_or(0).max(0);
    match persist
        .eval()
        .list_session_end_items(run_id, limit, offset)
        .await
    {
        Ok((items, total)) => Json(SessionEndItemsView {
            run_id,
            total,
            offset,
            limit,
            items,
        })
        .into_response(),
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("persistence error: {e}"),
        ),
    }
}
