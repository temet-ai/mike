use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::cache_breaks::reads::{self, BreakRow, CauseGroup};
use crate::state::AppState;
use crate::store::persistence::PersistHandle;

const DEFAULT_LIMIT: i64 = 100;
const MAX_LIMIT: i64 = 500;
const DEFAULT_HOURS: i64 = 24;

#[derive(Debug, Deserialize)]
pub(crate) struct Page {
    limit: Option<i64>,
    offset: Option<i64>,
    hours: Option<i64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct BreaksAnswer {
    session_id: Option<String>,
    since: DateTime<Utc>,
    groups: Vec<CauseGroup>,
    breaks: Vec<BreakRow>,
    limit: i64,
    offset: i64,
    has_more: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Rescan {
    days: Option<i64>,
}

fn persist(state: &AppState) -> Result<&PersistHandle, StatusCode> {
    state.persist().ok_or(StatusCode::SERVICE_UNAVAILABLE)
}

fn failed(e: impl std::fmt::Display) -> StatusCode {
    tracing::warn!(error = %e, "prompt cache breaks read failed");
    StatusCode::INTERNAL_SERVER_ERROR
}

async fn answer(
    persist: &PersistHandle,
    session: Option<(String, i64)>,
    since: DateTime<Utc>,
    page: &Page,
) -> Result<BreaksAnswer, StatusCode> {
    let limit = page.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = page.offset.unwrap_or(0).max(0);
    let db_id = session.as_ref().map(|(_, id)| *id);
    let groups = reads::groups(persist.pool(), db_id, since)
        .await
        .map_err(failed)?;
    let mut breaks = reads::rows(persist.pool(), db_id, since, limit + 1, offset)
        .await
        .map_err(failed)?;
    let has_more = breaks.len() as i64 > limit;
    breaks.truncate(usize::try_from(limit).unwrap_or_default());
    Ok(BreaksAnswer {
        session_id: session.map(|(uuid, _)| uuid),
        since,
        groups,
        breaks,
        limit,
        offset,
        has_more,
    })
}

pub(crate) async fn get_session_cache_breaks(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(page): Query<Page>,
) -> Result<Json<BreaksAnswer>, StatusCode> {
    let persist = persist(&state)?;
    let db_id = persist
        .resolve_session_db_id(&id)
        .await
        .map_err(failed)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let since = page.hours.map_or(DateTime::<Utc>::UNIX_EPOCH, |h| {
        Utc::now() - Duration::hours(h)
    });
    Ok(Json(
        answer(persist, Some((id, db_id)), since, &page).await?,
    ))
}

pub(crate) async fn get_cache_breaks(
    State(state): State<AppState>,
    Query(page): Query<Page>,
) -> Result<Json<BreaksAnswer>, StatusCode> {
    let persist = persist(&state)?;
    let since = Utc::now() - Duration::hours(page.hours.unwrap_or(DEFAULT_HOURS).max(1));
    Ok(Json(answer(persist, None, since, &page).await?))
}

pub(crate) async fn post_cache_breaks_rescan(
    State(state): State<AppState>,
    Json(body): Json<Rescan>,
) -> Result<StatusCode, StatusCode> {
    let persist = persist(&state)?;
    let days = body.days.unwrap_or(7).clamp(1, 30);
    crate::cache_breaks::rescan_from(persist, days)
        .await
        .map_err(failed)?;
    Ok(StatusCode::ACCEPTED)
}
