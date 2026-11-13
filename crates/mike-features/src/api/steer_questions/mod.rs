use crate::store::persistence::steerer_repo::SteererPersist as _;
pub mod actions;

pub use actions::{demote_question, label_evaluation, patch_question, promote_question};

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use crate::state::AppState;
use crate::steerer::guard::Question;
use crate::steerer::guard::promotion::status::{StatusRequest, questions_status};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::steering::promotion::EventsFilter;

const MAX_LIMIT: i64 = 200;

pub fn error(status: StatusCode, message: String) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

pub fn unavailable() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "session persistence is disabled".to_owned(),
    )
}

pub fn parse_question(raw: &str) -> Result<Question, Box<Response>> {
    Question::parse(raw).ok_or_else(|| {
        Box::new(error(
            StatusCode::BAD_REQUEST,
            format!("question {raw:?} is not one of relapse|unbacked_wait|authorised_handback|unobserved_done"),
        ))
    })
}

async fn verifier_id(persist: &PersistHandle) -> Option<i64> {
    persist
        .steerer()
        .list_steerers()
        .await
        .ok()?
        .into_iter()
        .find(|s| s.kind == "verifier" && s.name == "verifier")
        .map(|s| s.id)
}

pub async fn get_steer_questions(State(state): State<AppState>) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let Ok(Some(guard)) = persist.steerer().guard_steerer().await else {
        return error(
            StatusCode::NOT_FOUND,
            "the turn-guard steerer row is not seeded".to_owned(),
        );
    };
    let req = StatusRequest {
        steerer_id: guard.id,
        bundle_id: state.settings_overlay.load().bundles.steerer(guard.id),
        verifier_id: verifier_id(persist).await,
        now: chrono::Utc::now(),
    };
    match questions_status(persist, &req).await {
        Ok(body) => Json(body).into_response(),
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("persistence error: {e}"),
        ),
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct EventsQuery {
    pub arm: Option<String>,
    pub mode: Option<String>,
    pub cursor: Option<i64>,
    pub limit: Option<i64>,
}

pub async fn list_question_events(
    State(state): State<AppState>,
    Path((steerer_id, question)): Path<(i64, String)>,
    Query(q): Query<EventsQuery>,
) -> Response {
    let question = match parse_question(&question) {
        Ok(q) => q,
        Err(resp) => return *resp,
    };
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let limit = q.limit.unwrap_or(50).clamp(1, MAX_LIMIT);
    let rows = persist
        .steerer()
        .steer_question_events(&EventsFilter {
            steerer_id,
            question: question.code(),
            arm: q.arm.as_deref(),
            mode: q.mode.as_deref(),
            before_id: q.cursor,
            limit,
        })
        .await;
    let arm_rows = persist
        .steerer()
        .fetch_arm_rows(steerer_id, Some(question.code()), chrono::Utc::now())
        .await
        .unwrap_or_default();
    match rows {
        Ok(rows) => {
            let next = (i64::try_from(rows.len()).unwrap_or(0) == limit)
                .then(|| rows.last().map(|r| r.id))
                .flatten();
            let events: Vec<serde_json::Value> = rows
                .iter()
                .map(|r| {
                    let scored = arm_rows.iter().find(|a| a.id == r.id);
                    json!({"event": r, "scored_turn_id": scored.and_then(|a| a.scored_turn_id), "scored_reaction": scored.and_then(|a| a.scored_reaction.clone())})
                })
                .collect();
            Json(json!({"events": events, "next_cursor": next, "limit": limit})).into_response()
        }
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("persistence error: {e}"),
        ),
    }
}
