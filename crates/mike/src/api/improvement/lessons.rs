use crate::store::persistence::eval_repo::EvalPersist as _;
use crate::store::persistence::lessons_repo::LessonsPersist as _;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::api::eval::labels::spa_origin;
use crate::api::eval::projection::project_after_gold;
use crate::api::improvement::actions::{EffectView, lesson_effect};
use crate::eval::labels;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::eval_labels::{GoldEntry, GoldWrite, Subject};
use crate::store::persistence::lessons::proposals::ProposalError;
use crate::store::persistence::lessons::{
    LessonEvidenceRow, LessonInjectionSummary, LessonOutcome, LessonRetirement,
};

const REASONS: [&str; 4] = ["not_a_correction", "too_specific", "wrong", "other"];
const DEFAULT_LIMIT: i64 = 100;
const GOLD_FORBIDDEN: &str =
    "rejecting with not_a_correction writes a gold label from the mike UI only";

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": message.into() }))).into_response()
}

fn unavailable() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "session persistence is disabled; lesson candidates are not available",
    )
}

fn proposal_error(e: &ProposalError) -> Response {
    match e {
        ProposalError::NotFound => error(StatusCode::NOT_FOUND, "no such lesson candidate"),
        ProposalError::NotPending => {
            error(StatusCode::CONFLICT, "lesson is not a pending candidate")
        }
        ProposalError::Persist(p) => error(StatusCode::INTERNAL_SERVER_ERROR, p.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListParams {
    limit: Option<i64>,
}

pub(crate) async fn list_candidates(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let limit = params.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, 500);
    match persist.lessons().list_lesson_candidates(limit).await {
        Ok(rows) => Json(json!({ "candidates": rows })).into_response(),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct LessonDetail {
    #[serde(flatten)]
    candidate: crate::store::persistence::lessons::LessonCandidateRow,
    reinforcements: Vec<LessonEvidenceRow>,
    injections: LessonInjectionSummary,
    went_live_at: Option<chrono::DateTime<chrono::Utc>>,
    effect: Option<EffectView>,
    outcomes: LessonOutcome,
    #[serde(flatten)]
    retirement: LessonRetirement,
}

async fn lesson_retirement(persist: &PersistHandle, id: i64) -> LessonRetirement {
    persist
        .lessons()
        .lesson_retirement(id)
        .await
        .unwrap_or_else(|e| {
            log_detail_read_failed("retirement", id, &e);
            LessonRetirement::default()
        })
}

async fn lesson_outcomes(persist: &PersistHandle, id: i64) -> LessonOutcome {
    persist
        .lessons()
        .lesson_outcome(id)
        .await
        .unwrap_or_else(|e| {
            log_detail_read_failed("outcomes", id, &e);
            LessonOutcome {
                lesson_id: id,
                ..LessonOutcome::default()
            }
        })
}

fn log_detail_read_failed(
    part: &'static str,
    id: i64,
    e: &crate::store::persistence::PersistError,
) {
    tracing::warn!(lesson_id = id, part, error = %e, "improvement: lesson detail read failed");
}

async fn lesson_reinforcements(persist: &PersistHandle, id: i64) -> Vec<LessonEvidenceRow> {
    persist
        .lessons()
        .list_lesson_evidence(id)
        .await
        .unwrap_or_else(|e| {
            log_detail_read_failed("reinforcements", id, &e);
            Vec::new()
        })
}

async fn lesson_injections(persist: &PersistHandle, id: i64) -> LessonInjectionSummary {
    persist
        .lessons()
        .lesson_injection_stats(id)
        .await
        .unwrap_or_else(|e| {
            log_detail_read_failed("injections", id, &e);
            LessonInjectionSummary {
                count: 0,
                first_injected_at: None,
                last_injected_at: None,
            }
        })
}

async fn lesson_went_live(
    persist: &PersistHandle,
    id: i64,
) -> Option<chrono::DateTime<chrono::Utc>> {
    persist
        .lessons()
        .lesson_went_live_at(id)
        .await
        .unwrap_or_else(|e| {
            log_detail_read_failed("went_live_at", id, &e);
            None
        })
}

async fn lesson_effect_view(persist: &PersistHandle, id: i64) -> Option<EffectView> {
    match persist.eval().get_lesson_action(id).await {
        Ok(Some(row)) => Some(lesson_effect(&row)),
        Ok(None) => None,
        Err(e) => {
            log_detail_read_failed("effect", id, &e);
            None
        }
    }
}

pub(crate) async fn get_candidate(State(state): State<AppState>, Path(id): Path<i64>) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let candidate = match persist.lessons().get_lesson_candidate(id).await {
        Ok(Some(row)) => row,
        Ok(None) => return error(StatusCode::NOT_FOUND, "no such lesson candidate"),
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let detail = LessonDetail {
        reinforcements: lesson_reinforcements(persist, id).await,
        injections: lesson_injections(persist, id).await,
        went_live_at: lesson_went_live(persist, id).await,
        effect: lesson_effect_view(persist, id).await,
        outcomes: lesson_outcomes(persist, id).await,
        retirement: lesson_retirement(persist, id).await,
        candidate,
    };
    Json(json!({ "candidate": detail })).into_response()
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct ApproveBody {
    text: Option<String>,
}

pub(crate) async fn approve_candidate(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    body: Option<Json<ApproveBody>>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let text = body.and_then(|Json(b)| b.text);
    match persist
        .lessons()
        .approve_lesson_candidate(id, text.as_deref(), "operator")
        .await
    {
        Ok(()) => Json(json!({ "id": id, "stage": "daily" })).into_response(),
        Err(e) => proposal_error(&e),
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct RejectBody {
    reason: String,
    note: Option<String>,
}

fn validate_reject(body: &RejectBody) -> Option<Response> {
    if REASONS.contains(&body.reason.as_str()) {
        return None;
    }
    Some(error(
        StatusCode::BAD_REQUEST,
        format!("reason must be one of {REASONS:?}"),
    ))
}

async fn write_not_a_correction_label(
    state: &AppState,
    persist: &PersistHandle,
    headers: &HeaderMap,
    turn_id: i64,
    note: Option<&str>,
) -> Result<(), Response> {
    let Some(origin) = spa_origin(headers, state) else {
        return Err(error(StatusCode::FORBIDDEN, GOLD_FORBIDDEN));
    };
    let holdout = labels::holdout_percent(persist).await.unwrap_or(30);
    let entries = [GoldEntry {
        axis: "reaction".to_owned(),
        label: "accepted".to_owned(),
    }];
    let write = GoldWrite {
        subject: Subject::Turn { turn_id },
        entries: &entries,
        note,
        origin: &origin,
        holdout_percent: holdout,
        context: "improvement",
        eval_run_id: None,
    };
    match persist.eval().write_gold_labels(&write).await {
        Ok(written) => {
            project_after_gold(state, persist, &written).await;
            Ok(())
        }
        Err(e) => Err(error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

pub(crate) async fn reject_candidate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<RejectBody>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    if let Some(resp) = validate_reject(&body) {
        return resp;
    }
    if body.reason == "not_a_correction" {
        let Some(candidate) = persist
            .lessons()
            .get_lesson_candidate(id)
            .await
            .unwrap_or_default()
        else {
            return error(StatusCode::NOT_FOUND, "no such lesson candidate");
        };
        let Some(turn_id) = candidate.source_turn_id else {
            return error(
                StatusCode::CONFLICT,
                "this candidate has no originating turn to label",
            );
        };
        if let Err(resp) =
            write_not_a_correction_label(&state, persist, &headers, turn_id, body.note.as_deref())
                .await
        {
            return resp;
        }
    }
    match persist
        .lessons()
        .reject_lesson_candidate(id, &body.reason, body.note.as_deref())
        .await
    {
        Ok(()) => Json(json!({ "id": id, "stage": "archived" })).into_response(),
        Err(e) => proposal_error(&e),
    }
}
