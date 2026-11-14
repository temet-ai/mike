use crate::store::persistence::web_repo::WebPersist as _;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{bad_request, page_limit, refuse};
use crate::state::AppState;
use crate::store::persistence::wright::remedies::{ActionFilter, ActionRow, RemedyPatch};
use crate::web_base::{persist_error_response, persist_unavailable_response};
use crate::wright::remedies::Owners;
use crate::wright::remedies::undo::{UndoError, can_undo, undo};

const ACTIONS_DEFAULT_LIMIT: i64 = 50;
const LIMIT_RANGE: std::ops::RangeInclusive<i32> = 1..=1000;

#[derive(Debug, Serialize)]
pub(crate) struct ActionView {
    #[serde(flatten)]
    row: ActionRow,
    can_undo: bool,
}

impl From<ActionRow> for ActionView {
    fn from(row: ActionRow) -> Self {
        let can_undo = can_undo(&row);
        Self { row, can_undo }
    }
}

pub(crate) async fn list_remedies(State(state): State<AppState>) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let remedies = handle
        .web()
        .wright_remedies(None)
        .await
        .map_err(persist_error_response)?;
    Ok(Json(json!({"remedies": remedies})))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PatchRemedyBody {
    enabled: Option<bool>,
    daily_limit: Option<i32>,
    max_resources: Option<i32>,
}

fn in_range(label: &str, value: Option<i32>) -> Result<Option<i32>, String> {
    match value {
        Some(v) if !LIMIT_RANGE.contains(&v) => Err(format!(
            "{label} is {v}; it must be between {} and {}",
            LIMIT_RANGE.start(),
            LIMIT_RANGE.end()
        )),
        other => Ok(other),
    }
}

fn remedy_patch(body: PatchRemedyBody) -> Result<RemedyPatch, String> {
    let patch = RemedyPatch {
        enabled: body.enabled,
        daily_limit: in_range("daily_limit", body.daily_limit)?,
        max_resources: in_range("max_resources", body.max_resources)?,
    };
    if patch == RemedyPatch::default() {
        return Err(
            "the body changes nothing; send enabled, daily_limit or max_resources".to_owned(),
        );
    }
    Ok(patch)
}

pub(crate) async fn patch_remedy(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<PatchRemedyBody>,
) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let patch = remedy_patch(body).map_err(bad_request)?;
    let remedy = handle
        .web()
        .patch_wright_remedy(&name, &patch)
        .await
        .map_err(persist_error_response)?
        .ok_or_else(|| {
            refuse(
                StatusCode::NOT_FOUND,
                &format!("no Wright remedy is named `{name}`"),
            )
        })?;
    Ok(Json(json!({"remedy": remedy})))
}

#[derive(Debug, Deserialize)]
pub(crate) struct ActionsQuery {
    pub(super) problem_id: Option<i64>,
    pub(super) limit: Option<i64>,
    pub(super) before: Option<i64>,
}

pub(crate) async fn list_actions(
    State(state): State<AppState>,
    Query(query): Query<ActionsQuery>,
) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let filter = ActionFilter {
        problem_id: query.problem_id,
        before: query.before,
        limit: page_limit(query.limit, ACTIONS_DEFAULT_LIMIT, "limit").map_err(bad_request)?,
    };
    let rows = handle
        .web()
        .wright_actions(&filter)
        .await
        .map_err(persist_error_response)?;
    let full = i64::try_from(rows.len()).unwrap_or(i64::MAX) == filter.limit;
    let next_before = rows.last().filter(|_| full).map(|row| row.id);
    let actions: Vec<ActionView> = rows.into_iter().map(ActionView::from).collect();
    Ok(Json(
        json!({"actions": actions, "next_before": next_before}),
    ))
}

fn conflict(reason: &str) -> Response {
    (
        StatusCode::CONFLICT,
        Json(json!({"error": "conflict", "reason": reason})),
    )
        .into_response()
}

pub(crate) async fn undo_action(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, Response> {
    let owners = Owners::from_state(&state).ok_or_else(persist_unavailable_response)?;
    match undo(&owners, id).await {
        Ok(row) => Ok(Json(json!({"action": ActionView::from(row)}))),
        Err(UndoError::NotFound(id)) => Err(refuse(
            StatusCode::NOT_FOUND,
            &format!("no Wright action has id {id}"),
        )),
        Err(UndoError::Conflict(reason)) => Err(conflict(&reason)),
        Err(UndoError::Unavailable(reason)) => {
            Err(refuse(StatusCode::SERVICE_UNAVAILABLE, &reason))
        }
        Err(UndoError::Persist(e)) => Err(persist_error_response(e)),
    }
}
