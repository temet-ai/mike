use crate::store::persistence::web_repo::WebPersist as _;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{bad_request, page_limit, refuse};
use crate::state::AppState;
use crate::store::persistence::wright::RepairFilter;
use crate::web_base::{persist_error_response, persist_unavailable_response};
use crate::wright::repair::states;

const REPAIRS_DEFAULT_LIMIT: i64 = 50;

#[derive(Debug, Deserialize)]
pub(crate) struct RepairsQuery {
    problem_id: Option<i64>,
    limit: Option<i64>,
    before: Option<i64>,
}

fn positive(value: Option<i64>, label: &str) -> Result<Option<i64>, String> {
    match value {
        Some(n) if n <= 0 => Err(format!("{label} is {n}; it must be a repair id above 0")),
        other => Ok(other),
    }
}

pub(crate) async fn list_repairs(
    State(state): State<AppState>,
    Query(query): Query<RepairsQuery>,
) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let limit = page_limit(query.limit, REPAIRS_DEFAULT_LIMIT, "limit").map_err(bad_request)?;
    let filter = RepairFilter {
        problem_id: positive(query.problem_id, "problem_id").map_err(bad_request)?,
        before: positive(query.before, "before").map_err(bad_request)?,
        limit: limit + 1,
    };
    let mut repairs = handle
        .web()
        .list_wright_repairs(&filter)
        .await
        .map_err(persist_error_response)?;
    let more = repairs.len() > usize::try_from(limit).unwrap_or(usize::MAX);
    repairs.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
    let next_before = more
        .then(|| repairs.last().map(|repair| repair.id))
        .flatten();
    Ok(Json(json!({
        "repairs": repairs,
        "next_before": next_before,
        "limit": limit,
        "states": states::ALL,
    })))
}

pub(crate) async fn get_repair(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let repair = handle
        .web()
        .wright_repair(id)
        .await
        .map_err(persist_error_response)?
        .ok_or_else(|| {
            refuse(
                StatusCode::NOT_FOUND,
                &format!("no Wright repair has id {id}"),
            )
        })?;
    Ok(Json(json!({ "repair": repair })))
}
