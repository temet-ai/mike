use crate::store::persistence::web_repo::WebPersist as _;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{bad_request, page_limit, refuse};
use crate::state::AppState;
use crate::store::persistence::wright::{InvestigationRow, LimitsPatch, PageQuery};
use crate::web_base::{persist_error_response, persist_unavailable_response};
use crate::wright::investigate::RunMode;
use crate::wright::investigate::context::runs_dir;
use crate::wright::investigate::home::RunDir;

const DEFAULT_LIMIT: i64 = 50;

pub(crate) const DAILY_STARTS: (i32, i32) = (0, 24);
pub(crate) const WALL_SECONDS: (i32, i32) = (60, 7200);
pub(crate) const TOOL_CALLS: (i32, i32) = (1, 2000);
pub(crate) const TOKENS: (i64, i64) = (10_000, 50_000_000);

#[derive(Debug, Deserialize)]
pub(crate) struct PageParams {
    problem_id: Option<i64>,
    limit: Option<i64>,
    before: Option<i64>,
}

fn page(params: &PageParams) -> Result<PageQuery, Response> {
    let limit = page_limit(params.limit, DEFAULT_LIMIT, "limit").map_err(bad_request)?;
    if let Some(before) = params.before.filter(|b| *b < 1) {
        return Err(bad_request(format!(
            "before is {before}; it must be an id of 1 or more"
        )));
    }
    Ok(PageQuery {
        problem_id: params.problem_id,
        before: params.before,
        limit: limit + 1,
    })
}

fn paged<T: serde::Serialize>(
    mut rows: Vec<T>,
    limit: i64,
    id: impl Fn(&T) -> i64,
) -> (Vec<T>, Option<i64>) {
    let keep = usize::try_from(limit - 1).unwrap_or(usize::MAX);
    if rows.len() <= keep {
        return (rows, None);
    }
    rows.truncate(keep);
    let next = rows.last().map(id);
    (rows, next)
}

pub(crate) fn with_files(row: &InvestigationRow) -> Value {
    let files = match (runs_dir(), RunMode::from_code(&row.mode)) {
        (Ok(runs), Some(mode)) => RunDir::new(&runs, row.id).files(mode.output_file()),
        _ => Value::Null,
    };
    let mut value = serde_json::to_value(row).unwrap_or(Value::Null);
    if let Some(object) = value.as_object_mut() {
        object.insert("files".to_owned(), files);
    }
    value
}

pub(crate) async fn list_investigations(
    State(state): State<AppState>,
    Query(params): Query<PageParams>,
) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let query = page(&params)?;
    let rows = handle
        .web()
        .list_wright_runs(query)
        .await
        .map_err(persist_error_response)?;
    let (investigations, next_before) = paged(rows, query.limit, |r| r.id);
    let investigations: Vec<Value> = investigations.iter().map(with_files).collect();
    Ok(Json(json!({
        "investigations": investigations,
        "next_before": next_before,
    })))
}

pub(crate) async fn list_findings(
    State(state): State<AppState>,
    Query(params): Query<PageParams>,
) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let query = page(&params)?;
    let rows = handle
        .web()
        .list_wright_findings(query)
        .await
        .map_err(persist_error_response)?;
    let (findings, next_before) = paged(rows, query.limit, |r| r.id);
    Ok(Json(json!({
        "findings": findings,
        "next_before": next_before,
    })))
}

pub(crate) async fn get_finding(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let finding = handle
        .web()
        .wright_finding(id)
        .await
        .map_err(persist_error_response)?
        .ok_or_else(|| {
            refuse(
                StatusCode::NOT_FOUND,
                &format!("no Wright finding has id {id}"),
            )
        })?;
    let investigation = handle
        .web()
        .wright_run(finding.investigation_id)
        .await
        .map_err(persist_error_response)?
        .as_ref()
        .map(with_files);
    Ok(Json(json!({
        "finding": finding,
        "investigation": investigation,
    })))
}

fn ranges() -> Value {
    json!({
        "daily_starts": [DAILY_STARTS.0, DAILY_STARTS.1],
        "wall_seconds": [WALL_SECONDS.0, WALL_SECONDS.1],
        "tool_calls": [TOOL_CALLS.0, TOOL_CALLS.1],
        "tokens": [TOKENS.0, TOKENS.1],
    })
}

pub(crate) async fn get_limits(State(state): State<AppState>) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let limits = handle
        .web()
        .wright_engineer_limits()
        .await
        .map_err(persist_error_response)?;
    Ok(Json(json!({"limits": limits, "ranges": ranges()})))
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LimitsBody {
    pub(crate) daily_starts: Option<i32>,
    pub(crate) wall_seconds: Option<i32>,
    pub(crate) tool_calls: Option<i32>,
    pub(crate) tokens: Option<i64>,
}

fn within<T: PartialOrd + std::fmt::Display + Copy>(
    field: &str,
    value: Option<T>,
    (low, high): (T, T),
) -> Result<Option<T>, String> {
    match value {
        Some(v) if v < low || v > high => Err(format!(
            "{field} is {v}; it must be between {low} and {high}"
        )),
        other => Ok(other),
    }
}

pub(crate) fn validate_limits(body: &LimitsBody) -> Result<LimitsPatch, String> {
    let patch = LimitsPatch {
        daily_starts: within("daily_starts", body.daily_starts, DAILY_STARTS)?,
        wall_seconds: within("wall_seconds", body.wall_seconds, WALL_SECONDS)?,
        tool_calls: within("tool_calls", body.tool_calls, TOOL_CALLS)?,
        tokens: within("tokens", body.tokens, TOKENS)?,
    };
    if patch == LimitsPatch::default() {
        return Err(
            "the body names no limit; send one or more of daily_starts, wall_seconds, tool_calls, tokens"
                .to_owned(),
        );
    }
    Ok(patch)
}

pub(crate) async fn patch_limits_as_operator(
    state: State<AppState>,
    headers: axum::http::HeaderMap,
    body: Json<LimitsBody>,
) -> Result<Json<Value>, Response> {
    crate::api::wright::conversation::require_operator(&headers)?;
    patch_limits(state, body).await
}

pub(crate) async fn patch_limits(
    State(state): State<AppState>,
    Json(body): Json<LimitsBody>,
) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let patch = validate_limits(&body).map_err(bad_request)?;
    let limits = handle
        .web()
        .patch_wright_engineer_limits(patch)
        .await
        .map_err(persist_error_response)?;
    tracing::info!(?patch, "wright: engineer limits changed");
    Ok(Json(json!({"limits": limits, "ranges": ranges()})))
}
