use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;
use serde_json::json;

use super::{error, persist_error, unavailable};
use crate::semantic::BUILDER_VERSION;
use crate::state::AppState;
use crate::store::persistence::semantic::{
    AssignmentFilter, CandidateDecision, CandidateMemberRow, PatternCandidateRow,
};

const MAX_LIMIT: i64 = 500;
const SPA_ONLY: &str = "failure patterns are named or dismissed from the mike UI only";

fn page(limit: Option<i64>, offset: Option<i64>) -> (i64, i64) {
    (
        limit.unwrap_or(50).clamp(1, MAX_LIMIT),
        offset.unwrap_or(0).max(0),
    )
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct AssignmentsQuery {
    pub(crate) from: Option<DateTime<Utc>>,
    pub(crate) to: Option<DateTime<Utc>>,
    pub(crate) pattern: Option<String>,
    pub(crate) unassigned: Option<bool>,
    pub(crate) limit: Option<i64>,
    pub(crate) offset: Option<i64>,
}

pub(crate) async fn list_pattern_assignments(
    State(state): State<AppState>,
    Query(q): Query<AssignmentsQuery>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let (limit, offset) = page(q.limit, q.offset);
    let filter = AssignmentFilter {
        from: q.from,
        to: q.to,
        pattern: q.pattern.as_deref(),
        unassigned: q.unassigned.unwrap_or(false),
        limit,
        offset,
    };
    match persist.list_pattern_assignments(&filter).await {
        Ok((rows, total)) => {
            Json(json!({ "assignments": rows, "total": total, "limit": limit, "offset": offset }))
                .into_response()
        }
        Err(e) => persist_error(&e),
    }
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct CandidatesQuery {
    pub(crate) status: Option<String>,
    pub(crate) limit: Option<i64>,
    pub(crate) offset: Option<i64>,
}

fn candidate_view(c: &PatternCandidateRow, members: &[CandidateMemberRow]) -> serde_json::Value {
    let listed: Vec<serde_json::Value> = members
        .iter()
        .filter(|m| m.candidate_id == c.id)
        .map(|m| match &m.text {
            Some(_) => json!(m),
            None => json!({ "item_id": m.item_id, "status": "gone" }),
        })
        .collect();
    json!({ "candidate": c, "members": listed })
}

pub(crate) async fn list_pattern_candidates(
    State(state): State<AppState>,
    Query(q): Query<CandidatesQuery>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let (limit, offset) = page(q.limit, q.offset);
    match persist
        .list_pattern_candidates(q.status.as_deref(), limit, offset)
        .await
    {
        Ok((rows, members, total)) => {
            let candidates: Vec<serde_json::Value> =
                rows.iter().map(|c| candidate_view(c, &members)).collect();
            Json(json!({ "candidates": candidates, "total": total, "limit": limit, "offset": offset })).into_response()
        }
        Err(e) => persist_error(&e),
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct NameBody {
    pub(crate) pattern: String,
    pub(crate) description: String,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct DismissBody {
    pub(crate) note: Option<String>,
}

pub(crate) fn valid_slug(slug: &str) -> bool {
    (3..=64).contains(&slug.len())
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn decision_response(id: i64, d: CandidateDecision) -> Response {
    match d {
        CandidateDecision::NotFound => error(StatusCode::NOT_FOUND, format!("no pattern candidate with id {id}")),
        CandidateDecision::NotOpen(status) => error(StatusCode::CONFLICT, format!("candidate {id} is {status}, not open")),
        CandidateDecision::SlugConflict { existing_description } => error(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("that pattern name already exists with a different description: {existing_description}"),
        ),
        CandidateDecision::Named { sort_order, anchor_ids } => {
            Json(json!({ "candidate_id": id, "status": "named", "sort_order": sort_order, "anchor_ids": anchor_ids })).into_response()
        }
        CandidateDecision::Dismissed => Json(json!({ "candidate_id": id, "status": "dismissed" })).into_response(),
    }
}

pub(crate) async fn name_pattern_candidate(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Json(body): Json<NameBody>,
) -> Response {
    if crate::api::eval::labels::spa_origin(&headers, &state).is_none() {
        return error(StatusCode::FORBIDDEN, SPA_ONLY.to_owned());
    }
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    if !valid_slug(&body.pattern) || body.description.trim().is_empty() {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!(
                "pattern {:?} must match ^[a-z0-9-]{{3,64}}$ and carry a description",
                body.pattern
            ),
        );
    }
    match persist
        .name_pattern_candidate(id, &body.pattern, body.description.trim(), BUILDER_VERSION)
        .await
    {
        Ok(d) => decision_response(id, d),
        Err(e) => persist_error(&e),
    }
}

pub(crate) async fn dismiss_pattern_candidate(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    body: Option<Json<DismissBody>>,
) -> Response {
    if crate::api::eval::labels::spa_origin(&headers, &state).is_none() {
        return error(StatusCode::FORBIDDEN, SPA_ONLY.to_owned());
    }
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let note = body.and_then(|Json(b)| b.note).unwrap_or_default();
    match persist.dismiss_pattern_candidate(id, &note).await {
        Ok(d) => decision_response(id, d),
        Err(e) => persist_error(&e),
    }
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct DriftQuery {
    pub(crate) stream: Option<String>,
    pub(crate) from: Option<NaiveDate>,
    pub(crate) to: Option<NaiveDate>,
}

pub(crate) async fn get_embedding_drift(
    State(state): State<AppState>,
    Query(q): Query<DriftQuery>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let rows = match persist
        .list_drift_rows(q.stream.as_deref(), q.from, q.to)
        .await
    {
        Ok(rows) => rows,
        Err(e) => return persist_error(&e),
    };
    let alarms = persist
        .list_eval_alarms(&crate::store::persistence::eval_metrics::AlarmsFilter {
            state: crate::store::persistence::eval_metrics::AlarmState::All,
            kind: Some("embedding_drift".to_owned()),
            limit: MAX_LIMIT,
            ..Default::default()
        })
        .await;
    match alarms {
        Ok((alarms, alarm_total)) => {
            Json(json!({ "days": rows, "alarms": alarms, "alarm_total": alarm_total }))
                .into_response()
        }
        Err(e) => persist_error(&e),
    }
}
