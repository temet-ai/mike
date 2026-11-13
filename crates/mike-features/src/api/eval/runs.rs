use crate::store::persistence::eval_repo::EvalPersist as _;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use crate::state::AppState;
use crate::store::persistence::eval_runs::{
    Disagreement, EvalGateRow, EvalRunRow, ItemLabelFilter, RunItemsFilter, RunsFilter,
};

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 500;

#[derive(Debug, Deserialize, Default)]
pub struct RunsQuery {
    pub status: Option<String>,
    pub role: Option<String>,
    pub split: Option<String>,
    pub purpose: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ItemsQuery {
    pub outcome: Option<String>,
    pub correct: Option<bool>,
    pub reaction_correct: Option<bool>,
    pub delivery_correct: Option<bool>,
    pub disagreement: Option<String>,
    pub gold_reaction: Option<String>,
    pub gold_delivery: Option<String>,
    pub verdict_reaction: Option<String>,
    pub verdict_delivery: Option<String>,
    pub severity: Option<String>,
    pub reviewed: Option<bool>,
    pub include: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct GatesQuery {
    pub holdout_batch: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Serialize)]
pub struct ItemsPage<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub reviewed: i64,
    pub limit: i64,
    pub offset: i64,
}

pub fn error(status: StatusCode, message: String) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

pub fn unavailable() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "session persistence is disabled; eval runs are not available".to_owned(),
    )
}

fn page(limit: Option<i64>, offset: Option<i64>) -> Result<(i64, i64), Box<Response>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(Box::new(error(
            StatusCode::BAD_REQUEST,
            format!("limit must be between 1 and {MAX_LIMIT}, got {limit}"),
        )));
    }
    let offset = offset.unwrap_or(0);
    if offset < 0 {
        return Err(Box::new(error(
            StatusCode::BAD_REQUEST,
            format!("offset must be zero or more, got {offset}"),
        )));
    }
    Ok((limit, offset))
}

fn paged<T: Serialize>(
    result: Result<(Vec<T>, i64), crate::store::persistence::PersistError>,
    limit: i64,
    offset: i64,
) -> Response {
    match result {
        Ok((items, total)) => Json(Page {
            items,
            total,
            limit,
            offset,
        })
        .into_response(),
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("persistence error: {e}"),
        ),
    }
}

pub async fn list_eval_runs(State(state): State<AppState>, Query(q): Query<RunsQuery>) -> Response {
    let (limit, offset) = match page(q.limit, q.offset) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let filter = RunsFilter {
        status: q.status,
        role: q.role,
        split: q.split,
        purpose: q.purpose,
        limit,
        offset,
    };
    paged::<EvalRunRow>(persist.eval().list_eval_runs(&filter).await, limit, offset)
}

pub async fn get_eval_run(State(state): State<AppState>, Path(id): Path<i64>) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    match persist.eval().get_eval_run(id).await {
        Ok(Some(run)) => Json(run).into_response(),
        Ok(None) => error(StatusCode::NOT_FOUND, format!("no eval run with id {id}")),
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("persistence error: {e}"),
        ),
    }
}

fn includes(include: Option<&str>, part: &str) -> bool {
    include.is_some_and(|i| i.split(',').any(|p| p.trim() == part))
}

pub fn items_filter(
    q: ItemsQuery,
    limit: i64,
    offset: i64,
) -> Result<RunItemsFilter, Box<Response>> {
    if q.correct.is_some() && q.reaction_correct.is_some() && q.correct != q.reaction_correct {
        return Err(Box::new(error(
            StatusCode::BAD_REQUEST,
            "correct and reaction_correct disagree; send one".to_owned(),
        )));
    }
    let disagreement = match q.disagreement.as_deref() {
        None => None,
        Some(d) => Some(Disagreement::parse(d).ok_or_else(|| {
            Box::new(error(
                StatusCode::BAD_REQUEST,
                format!("disagreement must be any, reaction or delivery, got {d:?}"),
            ))
        })?),
    };
    Ok(RunItemsFilter {
        outcome: q.outcome,
        reaction_correct: q.reaction_correct.or(q.correct),
        delivery_correct: q.delivery_correct,
        disagreement,
        labels: ItemLabelFilter {
            gold_reaction: q.gold_reaction,
            gold_delivery: q.gold_delivery,
            verdict_reaction: q.verdict_reaction,
            verdict_delivery: q.verdict_delivery,
            severity: q.severity,
        },
        reviewed: q.reviewed,
        operator_turn_id: None,
        include_verdict: includes(q.include.as_deref(), "verdict"),
        include_review: includes(q.include.as_deref(), "review"),
        limit,
        offset,
    })
}

pub async fn list_eval_run_items(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(q): Query<ItemsQuery>,
) -> Response {
    let (limit, offset) = match page(q.limit, q.offset) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    let filter = match items_filter(q, limit, offset) {
        Ok(f) => f,
        Err(r) => return *r,
    };
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    match persist.eval().eval_run_items_page(id, &filter).await {
        Ok(p) => Json(ItemsPage {
            items: p.items,
            total: p.total,
            reviewed: p.reviewed,
            limit,
            offset,
        })
        .into_response(),
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("persistence error: {e}"),
        ),
    }
}

pub async fn list_eval_gates(
    State(state): State<AppState>,
    Query(q): Query<GatesQuery>,
) -> Response {
    let (limit, offset) = match page(q.limit, q.offset) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    paged::<EvalGateRow>(
        persist
            .eval()
            .list_eval_gates(q.holdout_batch.as_deref(), limit, offset)
            .await,
        limit,
        offset,
    )
}

pub async fn get_eval_gate(State(state): State<AppState>, Path(id): Path<i64>) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    match persist.eval().get_eval_gate(id).await {
        Ok(Some(gate)) => Json(gate).into_response(),
        Ok(None) => error(StatusCode::NOT_FOUND, format!("no eval gate with id {id}")),
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("persistence error: {e}"),
        ),
    }
}
