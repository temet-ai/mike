use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{page_limit, page_offset, refuse};
use crate::api::handlers::{persist_error_response, persist_unavailable_response};
use crate::config::AppState;
use crate::store::persistence::wright::{NewClientError, ProblemFilter};
use crate::wright::Source;
use crate::wright::watch::CHECKS;

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

fn state_filter(value: Option<&str>) -> Result<Option<String>, Response> {
    match value.map(str::trim).filter(|s| !s.is_empty()) {
        None | Some("all") => Ok(None),
        Some(state) if STATES.contains(&state) => Ok(Some(state.to_owned())),
        Some(other) => Err(refuse(
            StatusCode::BAD_REQUEST,
            &format!("state is `{other}`; it must be one of open, healthy or all"),
        )),
    }
}

fn check_filter(value: Option<&str>) -> Result<Option<String>, Response> {
    match value.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(name) if CHECKS.iter().any(|c| c.name == name) => Ok(Some(name.to_owned())),
        Some(other) => Err(refuse(
            StatusCode::BAD_REQUEST,
            &format!(
                "check is `{other}`; known checks: {}",
                CHECKS.iter().map(|c| c.name).collect::<Vec<_>>().join(", ")
            ),
        )),
    }
}

fn source_filter(value: Option<&str>) -> Result<Option<String>, Response> {
    match value.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(code) if Source::ALL.iter().any(|s| s.code() == code) => Ok(Some(code.to_owned())),
        Some(other) => Err(refuse(
            StatusCode::BAD_REQUEST,
            &format!(
                "source is `{other}`; known sources: {}",
                Source::ALL.map(Source::code).join(", ")
            ),
        )),
    }
}

fn watch_enabled(state: &AppState) -> bool {
    state.feature_toggle_overlay.load().wright
}

pub(crate) async fn list_problems(
    State(state): State<AppState>,
    Query(query): Query<ProblemsQuery>,
) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let filter = ProblemFilter {
        state: state_filter(query.state.as_deref())?,
        check: check_filter(query.check.as_deref())?,
        source: source_filter(query.source.as_deref())?,
        limit: page_limit(query.limit, PROBLEMS_DEFAULT_LIMIT, "limit")?,
        offset: page_offset(query.offset, "offset")?,
    };
    let page = handle
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
    let limit = page_limit(query.episode_limit, EPISODES_DEFAULT_LIMIT, "episode_limit")?;
    let offset = page_offset(query.episode_offset, "episode_offset")?;
    let problem = handle
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

pub(crate) async fn list_checks(State(state): State<AppState>) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let runs = handle
        .latest_wright_runs()
        .await
        .map_err(persist_error_response)?;
    let checks: Vec<Value> = CHECKS
        .iter()
        .map(|spec| {
            let latest = runs.iter().find(|run| run.check_name == spec.name);
            json!({
                "name": spec.name,
                "source": spec.source,
                "cadence": spec.cadence,
                "expects": spec.expects,
                "latest_run": latest,
            })
        })
        .collect();
    Ok(Json(json!({
        "checks": checks,
        "watch_enabled": watch_enabled(&state),
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

fn validate_client_error(body: ClientErrorBody) -> Result<NewClientError, Response> {
    let route = body.route.trim();
    let message = body.message.trim();
    if route.is_empty() || message.is_empty() {
        return Err(refuse(
            StatusCode::BAD_REQUEST,
            "route and message must both be non-empty",
        ));
    }
    if !CLIENT_ERROR_KINDS.contains(&body.kind.as_str()) {
        return Err(refuse(
            StatusCode::BAD_REQUEST,
            &format!(
                "kind is `{}`; it must be one of load, console, api or unhandled",
                body.kind
            ),
        ));
    }
    let size = route.len() + message.len() + body.detail.to_string().len();
    if size > CLIENT_ERROR_MAX_BYTES {
        return Err(refuse(
            StatusCode::PAYLOAD_TOO_LARGE,
            &format!("the report is {size} bytes; at most {CLIENT_ERROR_MAX_BYTES} are kept"),
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
    let error = validate_client_error(body)?;
    let id = handle
        .insert_wright_client_error(&error)
        .await
        .map_err(persist_error_response)?;
    Ok((StatusCode::ACCEPTED, Json(json!({"id": id}))).into_response())
}
