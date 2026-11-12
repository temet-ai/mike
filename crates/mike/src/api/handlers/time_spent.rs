use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::AppState;
use crate::store::persistence::time_spent::{
    BuildsRunningAtOnceRow, SlowCommandRow, TimeSpentByKindRow, TimeSpentFilter,
};
use crate::web_base::persist_unavailable_response;

pub(crate) const TIME_SPENT_DEFAULT_DAYS: i64 = 14;
pub(crate) const TIME_SPENT_MAX_DAYS: i64 = 90;
pub(crate) const TIME_SPENT_DEFAULT_LIMIT: i64 = 20;
pub(crate) const TIME_SPENT_MAX_LIMIT: i64 = 100;

pub(crate) const TIME_SPENT_NOTES: [&str; 5] = [
    "An answer's wait runs from the model finishing the answer to the first tool result reaching the next request. It includes permission prompts and every tool the answer called.",
    "An answer that called several kinds of tool is counted under the slowest kind.",
    "Waits over an hour are left out of the sums and counted separately.",
    "Builds running at once counts cargo, nextest, make and gate commands from other sessions in any project that overlapped each build, on every machine that writes to the capture database.",
    "Model time runs from a request being sent to its answer finishing.",
];

#[derive(Debug, Default, Deserialize)]
pub(crate) struct TimeSpentParams {
    #[serde(default)]
    pub(crate) project: Option<String>,
    #[serde(default)]
    pub(crate) project_path: Option<String>,
    #[serde(default)]
    pub(crate) from: Option<String>,
    #[serde(default)]
    pub(crate) to: Option<String>,
    #[serde(default)]
    pub(crate) days: Option<i64>,
    #[serde(default)]
    pub(crate) include_subagents: bool,
    #[serde(default)]
    pub(crate) session_id: Option<String>,
    #[serde(default)]
    pub(crate) kind: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<i64>,
    #[serde(default)]
    pub(crate) offset: Option<i64>,
}

#[derive(Debug, PartialEq)]
pub(crate) struct ResolvedTimeSpent {
    pub(crate) project: Option<String>,
    pub(crate) project_path: Option<String>,
    pub(crate) from: DateTime<Utc>,
    pub(crate) to: DateTime<Utc>,
    pub(crate) include_subagents: bool,
    pub(crate) session: Option<Uuid>,
    pub(crate) kind: Option<String>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct TimeSpentTotals {
    pub(crate) wait_seconds: f64,
    pub(crate) model_seconds: f64,
    pub(crate) answers: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct TimeSpentResponse {
    pub(crate) from: DateTime<Utc>,
    pub(crate) to: DateTime<Utc>,
    pub(crate) project: Option<String>,
    pub(crate) project_path: Option<String>,
    pub(crate) include_subagents: bool,
    pub(crate) session_id: Option<Uuid>,
    pub(crate) totals: TimeSpentTotals,
    pub(crate) by_kind: Vec<TimeSpentByKindRow>,
    pub(crate) slow_commands: Vec<SlowCommandRow>,
    pub(crate) builds_running_at_once: Vec<BuildsRunningAtOnceRow>,
    pub(crate) notes: [&'static str; 5],
}

fn non_blank(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn parse_rfc3339(raw: &str, field: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(&raw.trim().replace(' ', "+"))
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| format!("`{field}` must be RFC 3339 (got {raw:?}): {e}"))
}

fn resolve_window(
    params: &TimeSpentParams,
    now: DateTime<Utc>,
) -> Result<(DateTime<Utc>, DateTime<Utc>), String> {
    let to = match non_blank(params.to.as_deref()) {
        Some(raw) => parse_rfc3339(&raw, "to")?,
        None => now,
    };
    let from = match non_blank(params.from.as_deref()) {
        Some(raw) => parse_rfc3339(&raw, "from")?,
        None => {
            let days = params
                .days
                .unwrap_or(TIME_SPENT_DEFAULT_DAYS)
                .clamp(1, TIME_SPENT_MAX_DAYS);
            to - Duration::days(days)
        }
    };
    if from >= to {
        return Err(format!(
            "`from` ({}) must be earlier than `to` ({})",
            from.to_rfc3339(),
            to.to_rfc3339()
        ));
    }
    Ok((from, to))
}

pub(crate) fn resolve_time_spent(
    params: &TimeSpentParams,
    now: DateTime<Utc>,
) -> Result<ResolvedTimeSpent, String> {
    let (from, to) = resolve_window(params, now)?;
    let session = match non_blank(params.session_id.as_deref()) {
        Some(raw) => Some(
            Uuid::parse_str(&raw)
                .map_err(|e| format!("`session_id` must be a UUID (got {raw:?}): {e}"))?,
        ),
        None => None,
    };
    Ok(ResolvedTimeSpent {
        project: non_blank(params.project.as_deref()),
        project_path: non_blank(params.project_path.as_deref()),
        from,
        to,
        include_subagents: params.include_subagents,
        session,
        kind: non_blank(params.kind.as_deref()),
        limit: params
            .limit
            .unwrap_or(TIME_SPENT_DEFAULT_LIMIT)
            .clamp(1, TIME_SPENT_MAX_LIMIT),
        offset: params.offset.unwrap_or(0).max(0),
    })
}

pub(crate) fn build_time_spent_response(
    resolved: ResolvedTimeSpent,
    by_kind: Vec<TimeSpentByKindRow>,
    slow_commands: Vec<SlowCommandRow>,
    builds_running_at_once: Vec<BuildsRunningAtOnceRow>,
) -> TimeSpentResponse {
    let totals = TimeSpentTotals {
        wait_seconds: by_kind.iter().map(|r| r.wait_seconds).sum(),
        model_seconds: by_kind.iter().map(|r| r.model_seconds).sum(),
        answers: by_kind.iter().map(|r| r.answers).sum(),
    };
    TimeSpentResponse {
        from: resolved.from,
        to: resolved.to,
        project: resolved.project,
        project_path: resolved.project_path,
        include_subagents: resolved.include_subagents,
        session_id: resolved.session,
        totals,
        by_kind,
        slow_commands,
        builds_running_at_once,
        notes: TIME_SPENT_NOTES,
    }
}

pub(crate) async fn get_time_spent(
    State(state): State<AppState>,
    Query(params): Query<TimeSpentParams>,
) -> Result<Json<TimeSpentResponse>, Response> {
    let resolved = resolve_time_spent(&params, Utc::now())
        .map_err(|message| (StatusCode::BAD_REQUEST, message).into_response())?;
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let filter = TimeSpentFilter {
        project: resolved.project.as_deref(),
        project_path: resolved.project_path.as_deref(),
        from: resolved.from,
        to: resolved.to,
        include_subagents: resolved.include_subagents,
        session: resolved.session,
    };
    let (by_kind, slow_commands, builds_running_at_once) = tokio::try_join!(
        handle.time_spent_by_kind(&filter),
        handle.time_spent_slow_commands(
            &filter,
            resolved.limit,
            resolved.offset,
            resolved.kind.as_deref()
        ),
        handle.time_spent_builds_running_at_once(&filter),
    )
    .map_err(|e| {
        tracing::warn!(error = %e, "get_time_spent: query failed");
        (StatusCode::INTERNAL_SERVER_ERROR, "time_spent query failed").into_response()
    })?;
    Ok(Json(build_time_spent_response(
        resolved,
        by_kind,
        slow_commands,
        builds_running_at_once,
    )))
}

#[cfg(test)]
mod tests;
