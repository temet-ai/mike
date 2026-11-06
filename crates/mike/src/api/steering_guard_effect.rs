use std::collections::HashMap;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::config::AppState;
use crate::steerer::guard::Question;
use crate::steerer::guard::promotion::effect::{
    EffectEstimate, READ_POINT_HOLDBACK, estimate_effect,
};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::steering::promotion::InterventionFeedRow;

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

fn error(status: StatusCode, message: String) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

fn unavailable() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "session persistence is disabled; the turn guard is not available".to_owned(),
    )
}

fn persistence(e: &crate::store::persistence::PersistError) -> Response {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("persistence error: {e}"),
    )
}

async fn guard_steerer_id(persist: &PersistHandle) -> Result<i64, Response> {
    match persist.guard_steerer().await {
        Ok(Some(row)) => Ok(row.id),
        Ok(None) => Err(error(
            StatusCode::NOT_FOUND,
            "the turn-guard steerer row is not seeded".to_owned(),
        )),
        Err(e) => Err(persistence(&e)),
    }
}

async fn question_effect(
    persist: &PersistHandle,
    steerer_id: i64,
    question: Question,
    as_of_secs: i64,
) -> Result<EffectEstimate, crate::store::persistence::PersistError> {
    let now = Utc::now();
    let rows = persist
        .fetch_arm_rows(steerer_id, Some(question.code()), now)
        .await?;
    let turns: Vec<i64> = rows
        .iter()
        .filter(|r| r.scored_is_last)
        .filter_map(|r| r.scored_turn_id)
        .collect();
    let ends: HashMap<i64, String> = persist
        .session_end_labels_for_turns(&turns, now)
        .await?
        .into_iter()
        .map(|r| (r.end_turn_id, r.label))
        .collect();
    Ok(estimate_effect(
        &rows,
        &ends,
        Some(question.code()),
        as_of_secs,
    ))
}

pub(crate) async fn get_guard_effect(State(state): State<AppState>) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let steerer_id = match guard_steerer_id(persist).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    let as_of_secs = Utc::now().timestamp();
    let mut questions = Vec::with_capacity(Question::ALL.len());
    for question in Question::ALL {
        match question_effect(persist, steerer_id, question, as_of_secs).await {
            Ok(estimate) => questions.push(estimate),
            Err(e) => return persistence(&e),
        }
    }
    Json(serde_json::json!({
        "as_of_secs": as_of_secs,
        "read_point_holdback": READ_POINT_HOLDBACK,
        "questions": questions,
    }))
    .into_response()
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct InterventionsQuery {
    pub(crate) limit: Option<i64>,
    pub(crate) offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct InterventionsPage {
    pub(crate) interventions: Vec<InterventionFeedRow>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
}

pub(crate) async fn get_improvement_guard(
    State(state): State<AppState>,
    Query(query): Query<InterventionsQuery>,
) -> Response {
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return error(
            StatusCode::BAD_REQUEST,
            format!("limit must be between 1 and {MAX_LIMIT}, got {limit}"),
        );
    }
    let offset = query.offset.unwrap_or(0).max(0);
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let steerer_id = match guard_steerer_id(persist).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    match persist
        .guard_interventions_feed(steerer_id, limit, offset)
        .await
    {
        Ok(interventions) => Json(InterventionsPage {
            interventions,
            limit,
            offset,
        })
        .into_response(),
        Err(e) => persistence(&e),
    }
}
