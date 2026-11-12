use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::state::AppState;
use crate::store::persistence::work_threads::{
    CheckpointInput, EntryInput, HistoryPage, ListFilter, MetaUpdate, StateFields, WorkThreadError,
};
use crate::store::persistence::{PersistError, PersistHandle};
use crate::web_base::{persist_error_response, persist_unavailable_response};

const HISTORY_DEFAULT_LIMIT: i64 = 20;
const PAGE_MAX_LIMIT: i64 = 500;
const LIST_DEFAULT_LIMIT: i64 = 200;

fn refuse(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({"error": message}))).into_response()
}

fn thread_error(error: WorkThreadError) -> Response {
    match error {
        WorkThreadError::Invalid(message) => refuse(StatusCode::BAD_REQUEST, &message),
        WorkThreadError::NotFound(message) => refuse(StatusCode::NOT_FOUND, &message),
        WorkThreadError::Persist(PersistError::DataInvariant(message)) => {
            refuse(StatusCode::BAD_REQUEST, &message)
        }
        WorkThreadError::Persist(other) => persist_error_response(other),
    }
}

async fn placed_folder(handle: &PersistHandle, path: &str) -> Result<Option<i64>, Response> {
    if !path.starts_with('/') {
        return Ok(None);
    }
    let Ok(machine) = crate::machine_identity::load_default().await else {
        return Ok(None);
    };
    crate::workspace::checkouts::place_folder(
        handle,
        machine.machine_uuid,
        &machine.machine_name,
        path,
    )
    .await
    .map_err(persist_error_response)
}

async fn project_id(handle: &PersistHandle, selector: Option<&str>) -> Result<i64, Response> {
    let selector = selector.map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| {
        refuse(
            StatusCode::BAD_REQUEST,
            "project is required: give the project UUID or a registered path (list_project_locations shows both)",
        )
    })?;
    match handle.resolve_work_thread_project(selector).await {
        Ok(Some(project)) => Ok(project.id),
        Ok(None) => placed_folder(handle, selector).await?.ok_or_else(|| {
            refuse(
                StatusCode::BAD_REQUEST,
                &format!(
                    "Unknown project `{selector}`: no project has this path or this repository's \
                     git remote; use list_project_locations"
                ),
            )
        }),
        Err(other) => Err(thread_error(other)),
    }
}

fn bounded(value: Option<i64>, default: i64, label: &str) -> Result<i64, String> {
    let value = value.unwrap_or(default);
    (1..=PAGE_MAX_LIMIT)
        .contains(&value)
        .then_some(value)
        .ok_or_else(|| format!("{label} is {value}; it must be between 1 and {PAGE_MAX_LIMIT}"))
}

fn bad_request(message: String) -> Response {
    refuse(StatusCode::BAD_REQUEST, &message)
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListQuery {
    project: Option<String>,
    status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

pub(crate) async fn list_threads(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let project = project_id(handle, query.project.as_deref()).await?;
    let offset = query.offset.unwrap_or(0);
    if offset < 0 {
        return Err(refuse(
            StatusCode::BAD_REQUEST,
            &format!("offset is {offset}; it must be 0 or more"),
        ));
    }
    let filter = ListFilter {
        status: query
            .status
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
        limit: bounded(query.limit, LIST_DEFAULT_LIMIT, "limit").map_err(bad_request)?,
        offset,
    };
    let list = handle
        .list_work_threads(project, filter)
        .await
        .map_err(thread_error)?;
    Ok(Json(
        serde_json::to_value(list).map_err(|e| persist_error_response(e.into()))?,
    ))
}

#[derive(Debug, Deserialize)]
pub(crate) struct GetQuery {
    project: Option<String>,
    history_limit: Option<i64>,
    history_before: Option<i32>,
}

pub(crate) async fn get_thread(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(query): Query<GetQuery>,
) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let project = project_id(handle, query.project.as_deref()).await?;
    let page = HistoryPage {
        limit: bounded(query.history_limit, HISTORY_DEFAULT_LIMIT, "history_limit")
            .map_err(bad_request)?,
        before_ord: query.history_before,
    };
    let detail = handle
        .get_work_thread(project, &slug, page)
        .await
        .map_err(thread_error)?;
    Ok(Json(
        serde_json::to_value(detail).map_err(|e| persist_error_response(e.into()))?,
    ))
}

#[derive(Debug, Deserialize)]
pub(crate) struct ExportQuery {
    project: Option<String>,
}

pub(crate) async fn export_thread(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(query): Query<ExportQuery>,
) -> Result<Response, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let project = project_id(handle, query.project.as_deref()).await?;
    let detail = handle
        .get_work_thread(project, &slug, HistoryPage::everything())
        .await
        .map_err(thread_error)?;
    let markdown = crate::work_threads::export::render(&detail);
    Ok((
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        markdown,
    )
        .into_response())
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct MetaBody {
    title: Option<String>,
    goal: Option<String>,
    status: Option<String>,
    branch: Option<String>,
    worktree: Option<String>,
    depends_on: Option<Vec<String>>,
    acceptance_criteria: Option<String>,
}

impl MetaBody {
    fn as_update(&self) -> MetaUpdate<'_> {
        MetaUpdate {
            title: self.title.as_deref(),
            goal: self.goal.as_deref(),
            status: self.status.as_deref(),
            branch: self.branch.as_deref(),
            worktree: self.worktree.as_deref(),
            depends_on: self.depends_on.as_deref(),
            acceptance_criteria: self.acceptance_criteria.as_deref(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct CheckpointBody {
    project: Option<String>,
    slug: String,
    tldr: String,
    #[serde(default)]
    where_we_are: String,
    #[serde(default)]
    next_up: String,
    #[serde(default)]
    blockers: String,
    #[serde(default)]
    open_questions: String,
    #[serde(default)]
    decisions: String,
    #[serde(default)]
    files: Vec<String>,
    session_uuid: Option<Uuid>,
    commit_sha: Option<String>,
    contribution: Option<String>,
    #[serde(flatten)]
    meta: MetaBody,
}

pub(crate) async fn checkpoint_thread(
    State(state): State<AppState>,
    Json(body): Json<CheckpointBody>,
) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let project = project_id(handle, body.project.as_deref()).await?;
    let input = CheckpointInput {
        slug: &body.slug,
        tldr: &body.tldr,
        state: StateFields {
            where_we_are: &body.where_we_are,
            next_up: &body.next_up,
            blockers: &body.blockers,
            open_questions: &body.open_questions,
            decisions: &body.decisions,
            files: &body.files,
        },
        session_uuid: body.session_uuid,
        commit_sha: body.commit_sha.as_deref(),
        contribution: body.contribution.as_deref(),
        meta: body.meta.as_update(),
    };
    let outcome = handle
        .checkpoint_work_thread(project, &input)
        .await
        .map_err(thread_error)?;
    Ok(Json(
        serde_json::to_value(outcome).map_err(|e| persist_error_response(e.into()))?,
    ))
}

#[derive(Debug, Deserialize)]
pub(crate) struct EntryBody {
    project: Option<String>,
    kind: String,
    body: String,
    summary: Option<String>,
    session_uuid: Option<Uuid>,
    commit_sha: Option<String>,
    data: Option<Value>,
}

pub(crate) async fn add_entry(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Json(body): Json<EntryBody>,
) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let project = project_id(handle, body.project.as_deref()).await?;
    let input = EntryInput {
        slug: &slug,
        kind: &body.kind,
        body: &body.body,
        summary: body.summary.as_deref(),
        session_uuid: body.session_uuid,
        commit_sha: body.commit_sha.as_deref(),
        data: body.data.as_ref(),
    };
    let outcome = handle
        .add_work_thread_entry(project, &input)
        .await
        .map_err(thread_error)?;
    Ok(Json(
        serde_json::to_value(outcome).map_err(|e| persist_error_response(e.into()))?,
    ))
}

#[derive(Debug, Deserialize)]
pub(crate) struct PatchBody {
    project: Option<String>,
    #[serde(flatten)]
    meta: MetaBody,
}

pub(crate) async fn patch_thread(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Json(body): Json<PatchBody>,
) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let project = project_id(handle, body.project.as_deref()).await?;
    let identity = handle
        .update_work_thread_meta(project, &slug, &body.meta.as_update())
        .await
        .map_err(thread_error)?;
    Ok(Json(
        serde_json::to_value(identity).map_err(|e| persist_error_response(e.into()))?,
    ))
}
