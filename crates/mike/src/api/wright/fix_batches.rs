use crate::store::persistence::web_repo::WebPersist as _;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{bad_request, page_limit, refuse};
use crate::state::AppState;
use crate::web_base::{persist_error_response, persist_unavailable_response};
use crate::wright::repair::batch_states;

const DEFAULT_LIMIT: i64 = 30;

#[derive(Debug, Deserialize)]
pub(crate) struct BatchesQuery {
    limit: Option<i64>,
    before: Option<i64>,
}

pub(crate) async fn list_fix_batches(
    State(state): State<AppState>,
    Query(query): Query<BatchesQuery>,
) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let limit = page_limit(query.limit, DEFAULT_LIMIT, "limit").map_err(bad_request)?;
    if let Some(before) = query.before.filter(|b| *b < 1) {
        return Err(bad_request(format!(
            "before is {before}; it must be a batch id of 1 or more"
        )));
    }
    let mut batches = handle
        .web()
        .list_wright_fix_batches(query.before, limit + 1)
        .await
        .map_err(persist_error_response)?;
    let keep = usize::try_from(limit).unwrap_or(usize::MAX);
    let more = batches.len() > keep;
    batches.truncate(keep);
    let next_before = more.then(|| batches.last().map(|b| b.id)).flatten();
    Ok(Json(json!({
        "batches": batches,
        "next_before": next_before,
        "limit": limit,
        "states": batch_states::ALL,
    })))
}

pub(crate) async fn land_fix_batch_as_operator(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<Value>, Response> {
    crate::api::wright::conversation::require_operator(&headers)?;
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let web = handle.web();
    if let Some(batch) = web
        .close_wright_fix_batch_now(id)
        .await
        .map_err(persist_error_response)?
    {
        tracing::info!(
            batch = id,
            "wright: the operator closed a fix batch to land it now"
        );
        return Ok(Json(json!({
            "batch": batch,
            "landing": "closed; the fix host's next repair tick (within two minutes) gates and lands it",
        })));
    }
    let batch = web
        .wright_fix_batch(id)
        .await
        .map_err(persist_error_response)?
        .ok_or_else(|| {
            refuse(
                StatusCode::NOT_FOUND,
                &format!("no Wright fix batch has id {id}"),
            )
        })?;
    if batch.state == batch_states::OPEN {
        return Err(refuse(
            StatusCode::CONFLICT,
            &format!(
                "fix batch {id} is open but cannot close now: it holds no committed fix yet, or a repair or fixer is still working in its worktree"
            ),
        ));
    }
    Ok(Json(json!({
        "batch": batch,
        "landing": format!("already {}; nothing was changed", batch.state),
    })))
}
