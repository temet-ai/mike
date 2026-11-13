use crate::store::persistence::eval_repo::EvalPersist as _;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use crate::state::AppState;
use crate::store::persistence::eval_bundles::{BundleQuery, BundleRow};

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

#[derive(Debug, Deserialize)]
pub struct BundleListQuery {
    pub role: Option<String>,
    pub subject_key: Option<String>,
    pub origin: Option<String>,
    pub status: Option<String>,
    pub current: Option<bool>,
    pub before_id: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct BundleDto {
    #[serde(flatten)]
    pub row: BundleRow,
    pub steerer_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BundlePage {
    pub bundles: Vec<BundleDto>,
    pub next_before_id: Option<i64>,
}

impl From<BundleRow> for BundleDto {
    fn from(row: BundleRow) -> Self {
        let steerer_name = (row.role == "steerer")
            .then(|| {
                row.components
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
            })
            .flatten();
        Self { row, steerer_name }
    }
}

fn error(status: StatusCode, message: String) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

fn unavailable() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "session persistence is disabled; bundles are not available".to_owned(),
    )
}

pub fn page_of(mut rows: Vec<BundleRow>, limit: i64) -> BundlePage {
    let limit = usize::try_from(limit).unwrap_or(0);
    let has_more = rows.len() > limit;
    rows.truncate(limit);
    let next_before_id = if has_more {
        rows.last().map(|row| row.id)
    } else {
        None
    };
    BundlePage {
        bundles: rows.into_iter().map(BundleDto::from).collect(),
        next_before_id,
    }
}

pub async fn list_eval_bundles(
    State(state): State<AppState>,
    Query(query): Query<BundleListQuery>,
) -> Response {
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return error(
            StatusCode::BAD_REQUEST,
            format!("limit must be between 1 and {MAX_LIMIT}, got {limit}"),
        );
    }
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let filter = BundleQuery {
        role: query.role,
        subject_key: query.subject_key,
        origin: query.origin,
        status: query.status,
        current: query.current,
        before_id: query.before_id,
        limit: limit + 1,
    };
    match persist.eval().list_bundles(&filter).await {
        Ok(rows) => Json(page_of(rows, limit)).into_response(),
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("persistence error: {e}"),
        ),
    }
}

pub async fn get_eval_bundle(State(state): State<AppState>, Path(id): Path<i64>) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    match persist.eval().get_bundle(id).await {
        Ok(Some(row)) => Json(BundleDto::from(row)).into_response(),
        Ok(None) => error(StatusCode::NOT_FOUND, format!("no bundle with id {id}")),
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("persistence error: {e}"),
        ),
    }
}
