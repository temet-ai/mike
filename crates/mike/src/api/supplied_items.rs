use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::config::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::supplied_items::{HandCheckRow, SupplyFigure};

const DEFAULT_LIMIT: i64 = 100;
const MAX_LIMIT: i64 = 500;
const DEFAULT_SAMPLE: i64 = 150;
const MAX_SAMPLE: i64 = 200;
const MIN_SAMPLE: i64 = 100;
const MAX_DAYS: i64 = 90;
const VERDICTS: [&str; 3] = ["used", "corrected_anyway", "ignored"];

#[derive(Debug, Default, Deserialize)]
pub(crate) struct ListQuery {
    request_id: Option<i64>,
    session_id: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct FiguresQuery {
    days: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct SampleQuery {
    week: Option<String>,
    size: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct HandCheck {
    verdict: String,
    checked_by: String,
}

fn refuse(status: StatusCode, message: String) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn unavailable() -> Response {
    refuse(
        StatusCode::SERVICE_UNAVAILABLE,
        "persistence is disabled".to_owned(),
    )
}

fn persist(state: &AppState) -> Option<&PersistHandle> {
    state.persist()
}

fn failed(error: impl std::fmt::Display) -> Response {
    refuse(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

pub(crate) async fn list_supplied_items(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Response {
    let Some(persist) = persist(&state) else {
        return unavailable();
    };
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = query.offset.unwrap_or(0).max(0);
    let rows = match (query.request_id, query.session_id.as_deref()) {
        (Some(request_id), _) => {
            persist
                .supplied_items_for_request(request_id, limit + 1, offset)
                .await
        }
        (None, Some(session)) => match Uuid::parse_str(session) {
            Ok(session) => {
                persist
                    .supplied_items_for_session(session, limit + 1, offset)
                    .await
            }
            Err(error) => {
                return refuse(
                    StatusCode::BAD_REQUEST,
                    format!("session_id `{session}` is not a UUID: {error}"),
                );
            }
        },
        (None, None) => {
            return refuse(
                StatusCode::BAD_REQUEST,
                "give request_id (a request id) or session_id (a session UUID)".to_owned(),
            );
        }
    };
    let mut rows = match rows {
        Ok(rows) => rows,
        Err(error) => return failed(error),
    };
    let has_more = rows.len() as i64 > limit;
    rows.truncate(limit as usize);
    let next_offset = has_more.then_some(offset + limit);
    Json(json!({
        "items": rows,
        "has_more": has_more,
        "next_offset": next_offset,
    }))
    .into_response()
}

pub(crate) async fn supply_figures(
    State(state): State<AppState>,
    Query(query): Query<FiguresQuery>,
) -> Response {
    let Some(persist) = persist(&state) else {
        return unavailable();
    };
    let days = query.days.unwrap_or(7).clamp(1, MAX_DAYS);
    let since = Utc::now() - Duration::days(days);
    match persist.supply_figures(since).await {
        Ok(rows) => Json(figures_answer(since, days, &rows)).into_response(),
        Err(error) => failed(error),
    }
}

pub(crate) fn figures_answer(since: DateTime<Utc>, days: i64, rows: &[SupplyFigure]) -> Value {
    let sum = |f: fn(&SupplyFigure) -> i64| rows.iter().map(f).sum::<i64>();
    let judged = sum(|r| r.judged);
    let ignored = sum(|r| r.ignored);
    json!({
        "since": since,
        "days": days,
        "by_kind_and_route": rows,
        "totals": {
            "supplied": sum(|r| r.supplied),
            "bytes": sum(|r| r.bytes),
            "judged": judged,
            "used": sum(|r| r.used),
            "corrected_anyway": sum(|r| r.corrected_anyway),
            "ignored": ignored,
            "bytes_ignored": sum(|r| r.bytes_ignored),
            "unused_share": (judged > 0).then(|| ignored as f64 / judged as f64),
        },
        "cost": "an item judged ignored was sent for nothing: bytes_ignored is the cost",
    })
}

pub(crate) async fn hand_check_sample(
    State(state): State<AppState>,
    Query(query): Query<SampleQuery>,
) -> Response {
    let Some(persist) = persist(&state) else {
        return unavailable();
    };
    let week = match week_start(query.week.as_deref()) {
        Ok(week) => week,
        Err(message) => return refuse(StatusCode::BAD_REQUEST, message),
    };
    let size = query
        .size
        .unwrap_or(DEFAULT_SAMPLE)
        .clamp(MIN_SAMPLE, MAX_SAMPLE);
    match persist.supply_hand_sample(week, size).await {
        Ok(rows) => Json(sample_answer(week, size, &rows)).into_response(),
        Err(error) => failed(error),
    }
}

pub(crate) fn week_start(week: Option<&str>) -> Result<DateTime<Utc>, String> {
    let day = match week {
        Some(text) => NaiveDate::parse_from_str(text, "%Y-%m-%d")
            .map_err(|error| format!("week `{text}` is not a date (YYYY-MM-DD): {error}"))?,
        None => Utc::now().date_naive(),
    };
    let monday = day
        - Duration::days(i64::from(
            chrono::Datelike::weekday(&day).num_days_from_monday(),
        ));
    monday
        .and_hms_opt(0, 0, 0)
        .map(|t| t.and_utc())
        .ok_or_else(|| format!("week `{day}` has no midnight"))
}

pub(crate) fn sample_answer(week: DateTime<Utc>, size: i64, rows: &[HandCheckRow]) -> Value {
    let checked: Vec<_> = rows.iter().filter(|r| r.hand_verdict.is_some()).collect();
    let agreed = checked
        .iter()
        .filter(|r| r.hand_verdict.as_deref() == Some(r.verdict.as_str()))
        .count();
    json!({
        "week_start": week,
        "size": size,
        "sample": rows,
        "checked": checked.len(),
        "agreed": agreed,
        "agreement_rate": (!checked.is_empty()).then(|| agreed as f64 / checked.len() as f64),
    })
}

pub(crate) async fn post_hand_check(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<HandCheck>,
) -> Response {
    let Some(persist) = persist(&state) else {
        return unavailable();
    };
    if !VERDICTS.contains(&body.verdict.as_str()) {
        return refuse(
            StatusCode::BAD_REQUEST,
            format!(
                "verdict `{}` is not one of used, corrected_anyway, ignored",
                body.verdict
            ),
        );
    }
    if body.checked_by.trim().is_empty() {
        return refuse(
            StatusCode::BAD_REQUEST,
            "checked_by is empty: name the person who checked".to_owned(),
        );
    }
    match persist
        .set_supply_hand_verdict(id, &body.verdict, body.checked_by.trim())
        .await
    {
        Ok(true) => {
            Json(json!({ "supplied_item_id": id, "hand_verdict": body.verdict })).into_response()
        }
        Ok(false) => refuse(
            StatusCode::NOT_FOUND,
            format!("supplied item {id} has no verdict to check"),
        ),
        Err(error) => failed(error),
    }
}

#[cfg(test)]
#[path = "supplied_items_tests.rs"]
mod supplied_items_tests;
