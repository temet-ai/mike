use crate::store::persistence::web_repo::WebPersist as _;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{bad_request, page_limit_up_to};
use crate::state::AppState;
use crate::store::persistence::wright::{Fixer, FollowupLimitsPatch, FollowupRow};
use crate::web_base::{persist_error_response, persist_unavailable_response};

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

pub(crate) const STATES: [&str; 7] = [
    "open",
    "filed",
    "handed_off",
    "batched",
    "fixed",
    "dropped",
    "stuck",
];
pub(crate) const DAILY_FILED: (i32, i32) = (0, 20);
pub(crate) const DAILY_HANDOFFS: (i32, i32) = (0, 10);
pub(crate) const MAX_ATTEMPTS: (i32, i32) = (1, 5);
pub(crate) const RECURRENCE_ACTIONS: (i32, i32) = (2, 1000);
pub(crate) const RECURRENCE_DAYS: (i32, i32) = (1, 30);

#[derive(Debug, Deserialize)]
pub(crate) struct FollowupParams {
    state: Option<String>,
    limit: Option<i64>,
    before: Option<i64>,
}

pub(crate) struct FollowupPage {
    pub(crate) state: Option<String>,
    pub(crate) limit: i64,
    pub(crate) before: Option<i64>,
}

pub(crate) fn followup_page(params: &FollowupParams) -> Result<FollowupPage, String> {
    let limit = page_limit_up_to(params.limit, DEFAULT_LIMIT, MAX_LIMIT, "limit")?;
    if let Some(before) = params.before.filter(|b| *b < 1) {
        return Err(format!("before is {before}; it must be an id of 1 or more"));
    }
    if let Some(state) = params.state.as_deref().filter(|s| !STATES.contains(s)) {
        return Err(format!(
            "state is `{state}`; it must be one of {}",
            STATES.join(", ")
        ));
    }
    Ok(FollowupPage {
        state: params.state.clone(),
        limit,
        before: params.before,
    })
}

pub(crate) fn paged(mut rows: Vec<FollowupRow>, limit: i64) -> (Vec<FollowupRow>, Option<i64>) {
    let keep = usize::try_from(limit).unwrap_or(usize::MAX);
    if rows.len() <= keep {
        return (rows, None);
    }
    rows.truncate(keep);
    let next = rows.last().map(|row| row.id);
    (rows, next)
}

pub(crate) async fn list_followups(
    State(state): State<AppState>,
    Query(params): Query<FollowupParams>,
) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let page = followup_page(&params).map_err(bad_request)?;
    let rows = handle
        .web()
        .list_wright_followups(page.state.as_deref(), page.limit + 1, page.before)
        .await
        .map_err(persist_error_response)?;
    let (followups, next_before) = paged(rows, page.limit);
    Ok(Json(json!({
        "followups": followups,
        "next_before": next_before,
    })))
}

fn ranges() -> Value {
    json!({
        "daily_filed": [DAILY_FILED.0, DAILY_FILED.1],
        "daily_handoffs": [DAILY_HANDOFFS.0, DAILY_HANDOFFS.1],
        "max_attempts": [MAX_ATTEMPTS.0, MAX_ATTEMPTS.1],
        "recurrence_actions": [RECURRENCE_ACTIONS.0, RECURRENCE_ACTIONS.1],
        "recurrence_days": [RECURRENCE_DAYS.0, RECURRENCE_DAYS.1],
    })
}

pub(crate) async fn get_followup_limits(
    State(state): State<AppState>,
) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let limits = handle
        .web()
        .wright_followup_limits()
        .await
        .map_err(persist_error_response)?;
    Ok(Json(json!({"limits": limits, "ranges": ranges()})))
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FollowupLimitsBody {
    pub(crate) repo: Option<String>,
    pub(crate) daily_filed: Option<i32>,
    pub(crate) daily_handoffs: Option<i32>,
    pub(crate) max_attempts: Option<i32>,
    pub(crate) recurrence_actions: Option<i32>,
    pub(crate) recurrence_days: Option<i32>,
    pub(crate) fixers: Option<Vec<Fixer>>,
    pub(crate) fix_host: Option<String>,
}

fn check_fix_host(host: Option<&str>) -> Result<(), String> {
    match host {
        Some(host) if host.trim().is_empty() || host.trim() != host => Err(format!(
            "fix_host is {host:?}; it must be a machine name with no surrounding spaces, such as omit"
        )),
        _ => Ok(()),
    }
}

fn within(field: &str, value: Option<i32>, (low, high): (i32, i32)) -> Result<Option<i32>, String> {
    match value {
        Some(v) if v < low || v > high => Err(format!(
            "{field} is {v}; it must be between {low} and {high}"
        )),
        other => Ok(other),
    }
}

fn check_repo(repo: Option<&str>) -> Result<(), String> {
    match repo {
        Some(repo)
            if repo
                .split_once('/')
                .is_none_or(|(o, n)| o.is_empty() || n.is_empty()) =>
        {
            Err(format!("repo is `{repo}`; it must look like owner/name"))
        }
        _ => Ok(()),
    }
}

fn check_fixers(fixers: Option<&[Fixer]>) -> Result<(), String> {
    let blank = |value: &str| value.trim().is_empty();
    for (n, fixer) in fixers.unwrap_or_default().iter().enumerate() {
        if blank(&fixer.machine)
            || fixer.host.as_deref().is_some_and(blank)
            || fixer.folder.as_deref().is_some_and(blank)
        {
            return Err(format!(
                "fixers[{n}] is {}; it needs a machine name, and its host and folder are null or not blank",
                json!(fixer)
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_followup_limits(
    body: FollowupLimitsBody,
) -> Result<FollowupLimitsPatch, String> {
    check_repo(body.repo.as_deref())?;
    check_fixers(body.fixers.as_deref())?;
    check_fix_host(body.fix_host.as_deref())?;
    let patch = FollowupLimitsPatch {
        repo: body.repo,
        daily_filed: within("daily_filed", body.daily_filed, DAILY_FILED)?,
        daily_handoffs: within("daily_handoffs", body.daily_handoffs, DAILY_HANDOFFS)?,
        max_attempts: within("max_attempts", body.max_attempts, MAX_ATTEMPTS)?,
        recurrence_actions: within(
            "recurrence_actions",
            body.recurrence_actions,
            RECURRENCE_ACTIONS,
        )?,
        recurrence_days: within("recurrence_days", body.recurrence_days, RECURRENCE_DAYS)?,
        fixers: body.fixers,
        fix_host: body.fix_host,
    };
    if patch == FollowupLimitsPatch::default() {
        return Err(
            "the body names no limit; send one or more of repo, daily_filed, daily_handoffs, max_attempts, recurrence_actions, recurrence_days, fixers, fix_host"
                .to_owned(),
        );
    }
    Ok(patch)
}

pub(crate) async fn patch_followup_limits_as_operator(
    state: State<AppState>,
    headers: HeaderMap,
    body: Json<FollowupLimitsBody>,
) -> Result<Json<Value>, Response> {
    crate::api::wright::conversation::require_operator(&headers)?;
    patch_followup_limits(state, body).await
}

pub(crate) async fn patch_followup_limits(
    State(state): State<AppState>,
    Json(body): Json<FollowupLimitsBody>,
) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let patch = validate_followup_limits(body).map_err(bad_request)?;
    let limits = handle
        .web()
        .patch_wright_followup_limits(&patch)
        .await
        .map_err(persist_error_response)?;
    tracing::info!(?patch, "wright: follow-up limits changed");
    Ok(Json(json!({"limits": limits, "ranges": ranges()})))
}
