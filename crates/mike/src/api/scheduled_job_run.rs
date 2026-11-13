use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::api::eval::labels::spa_origin;
use crate::scheduled_jobs::{RefusalReason, RunRefusal, request_run};
use crate::state::AppState;

#[cfg(test)]
#[path = "scheduled_job_run_tests.rs"]
mod scheduled_job_run_tests;

const FROM_APP_ONLY: &str = "scheduled jobs are started by hand from the mike web app only (the Scheduled jobs panel on the home page)";

fn refused(status: StatusCode, reason: &str, message: &str) -> Response {
    (status, Json(json!({ "error": message, "reason": reason }))).into_response()
}

fn refusal_response(refusal: &RunRefusal) -> Response {
    let (status, reason) = match refusal.reason {
        RefusalReason::UnknownJob => (StatusCode::NOT_FOUND, "unknown_job"),
        RefusalReason::NotStarted => (StatusCode::CONFLICT, "not_started"),
        RefusalReason::TurnedOff => (StatusCode::CONFLICT, "turned_off"),
        RefusalReason::NoScripts => (StatusCode::CONFLICT, "no_scripts"),
        RefusalReason::HeldElsewhere => (StatusCode::CONFLICT, "held_elsewhere"),
        RefusalReason::AlreadyRunning => (StatusCode::CONFLICT, "already_running"),
    };
    refused(status, reason, &refusal.message)
}

pub(crate) async fn post_run_scheduled_job(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Response {
    if spa_origin(&headers, &state).is_none() {
        return refused(StatusCode::FORBIDDEN, "not_from_app", FROM_APP_ONLY);
    }
    match request_run(&name, chrono::Utc::now()) {
        Ok(accepted) => (StatusCode::ACCEPTED, Json(accepted)).into_response(),
        Err(refusal) => refusal_response(&refusal),
    }
}
