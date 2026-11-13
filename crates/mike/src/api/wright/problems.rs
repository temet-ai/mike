use crate::store::persistence::web_repo::WebPersist as _;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{bad_request, page_limit, page_offset, refuse};
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::wright::{NewClientError, ProblemFilter};
use crate::web_base::{persist_error_response, persist_unavailable_response};
use crate::wright::Source;

const PROBLEMS_DEFAULT_LIMIT: i64 = 50;
const EPISODES_DEFAULT_LIMIT: i64 = 50;
const STATES: [&str; 3] = ["open", "healthy", "all"];
const CLIENT_ERROR_KINDS: [&str; 4] = ["load", "console", "api", "unhandled"];
const CLIENT_ERROR_MAX_BYTES: usize = 64 * 1024;

#[derive(Debug, Deserialize)]
pub(crate) struct ProblemsQuery {
    state: Option<String>,
    check: Option<String>,
    source: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

fn state_filter(value: Option<&str>) -> Result<Option<String>, String> {
    match value.map(str::trim).filter(|s| !s.is_empty()) {
        None | Some("all") => Ok(None),
        Some(state) if STATES.contains(&state) => Ok(Some(state.to_owned())),
        Some(other) => Err(format!(
            "state is `{other}`; it must be one of open, healthy or all"
        )),
    }
}

async fn check_filter(
    handle: &PersistHandle,
    value: Option<&str>,
) -> Result<Option<String>, Response> {
    let Some(name) = value.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let known: Vec<String> = handle
        .web()
        .list_wright_checks()
        .await
        .map_err(persist_error_response)?
        .into_iter()
        .map(|check| check.name)
        .collect();
    if known.iter().any(|known| known == name) {
        return Ok(Some(name.to_owned()));
    }
    Err(bad_request(format!(
        "check is `{name}`; known checks: {}",
        known.join(", ")
    )))
}

fn source_filter(value: Option<&str>) -> Result<Option<String>, String> {
    match value.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(code) if Source::ALL.iter().any(|s| s.code() == code) => Ok(Some(code.to_owned())),
        Some(other) => Err(format!(
            "source is `{other}`; known sources: {}",
            Source::ALL.map(Source::code).join(", ")
        )),
    }
}

async fn problem_filter(
    handle: &PersistHandle,
    query: &ProblemsQuery,
) -> Result<ProblemFilter, Response> {
    Ok(ProblemFilter {
        state: state_filter(query.state.as_deref()).map_err(bad_request)?,
        check: check_filter(handle, query.check.as_deref()).await?,
        source: source_filter(query.source.as_deref()).map_err(bad_request)?,
        limit: page_limit(query.limit, PROBLEMS_DEFAULT_LIMIT, "limit").map_err(bad_request)?,
        offset: page_offset(query.offset, "offset").map_err(bad_request)?,
    })
}

fn watch_enabled(state: &AppState) -> bool {
    state.feature_toggle_overlay.load().wright
}

pub(crate) async fn list_problems(
    State(state): State<AppState>,
    Query(query): Query<ProblemsQuery>,
) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let filter = problem_filter(handle, &query).await?;
    let page = handle
        .web()
        .list_wright_problems(&filter)
        .await
        .map_err(persist_error_response)?;
    let shown = i64::try_from(page.problems.len()).unwrap_or(i64::MAX);
    let next_offset = (filter.offset + shown < page.total).then_some(filter.offset + shown);
    Ok(Json(json!({
        "problems": page.problems,
        "total": page.total,
        "limit": filter.limit,
        "offset": filter.offset,
        "next_offset": next_offset,
        "watch_enabled": watch_enabled(&state),
    })))
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProblemQuery {
    episode_limit: Option<i64>,
    episode_offset: Option<i64>,
}

pub(crate) async fn get_problem(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(query): Query<ProblemQuery>,
) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let limit = page_limit(query.episode_limit, EPISODES_DEFAULT_LIMIT, "episode_limit")
        .map_err(bad_request)?;
    let offset = page_offset(query.episode_offset, "episode_offset").map_err(bad_request)?;
    let problem = handle
        .web()
        .wright_problem(id)
        .await
        .map_err(persist_error_response)?
        .ok_or_else(|| {
            refuse(
                StatusCode::NOT_FOUND,
                &format!("no Wright problem has id {id}"),
            )
        })?;
    let episodes = handle
        .web()
        .wright_episodes(id, limit, offset)
        .await
        .map_err(persist_error_response)?;
    let shown = i64::try_from(episodes.len()).unwrap_or(i64::MAX);
    let next_offset = (offset + shown < problem.episodes).then_some(offset + shown);
    Ok(Json(json!({
        "problem": problem,
        "episodes": episodes,
        "episode_limit": limit,
        "episode_offset": offset,
        "next_episode_offset": next_offset,
    })))
}

#[derive(Debug, Deserialize)]
pub(crate) struct ClientErrorBody {
    route: String,
    kind: String,
    message: String,
    #[serde(default)]
    detail: Value,
}

fn validate_client_error(body: ClientErrorBody) -> Result<NewClientError, (StatusCode, String)> {
    let route = body.route.trim();
    let message = body.message.trim();
    if route.is_empty() || message.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "route and message must both be non-empty".to_owned(),
        ));
    }
    if !CLIENT_ERROR_KINDS.contains(&body.kind.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "kind is `{}`; it must be one of load, console, api or unhandled",
                body.kind
            ),
        ));
    }
    let size = route.len() + message.len() + body.detail.to_string().len();
    if size > CLIENT_ERROR_MAX_BYTES {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("the report is {size} bytes; at most {CLIENT_ERROR_MAX_BYTES} are kept"),
        ));
    }
    let detail = if body.detail.is_null() {
        json!({})
    } else {
        body.detail
    };
    Ok(NewClientError {
        route: route.to_owned(),
        kind: body.kind,
        message: message.to_owned(),
        detail,
    })
}

pub(crate) async fn report_client_error(
    State(state): State<AppState>,
    Json(body): Json<ClientErrorBody>,
) -> Result<Response, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let error =
        validate_client_error(body).map_err(|(status, message)| refuse(status, &message))?;
    let id = handle
        .web()
        .insert_wright_client_error(&error)
        .await
        .map_err(persist_error_response)?;
    Ok((StatusCode::ACCEPTED, Json(json!({"id": id}))).into_response())
}
