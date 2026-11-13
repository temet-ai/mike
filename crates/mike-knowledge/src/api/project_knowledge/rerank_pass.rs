use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

use crate::rerank_nightly::{RunNow, run_pass_now};
use crate::semantic::KnowledgeState;

pub async fn rerank_pass(State(state): State<KnowledgeState>) -> Response {
    match run_pass_now(&state).await {
        RunNow::Ran(body) => (StatusCode::OK, Json(body)).into_response(),
        RunNow::NoDatabase => refused(StatusCode::SERVICE_UNAVAILABLE, "mike has no database"),
        RunNow::TuningOff => refused(StatusCode::CONFLICT, "rerank.tuning.enabled is off"),
        RunNow::OtherMachine => refused(
            StatusCode::CONFLICT,
            "another machine holds the rerank_nightly lease; run it there",
        ),
    }
}

fn refused(status: StatusCode, reason: &str) -> Response {
    (status, Json(json!({"error": reason}))).into_response()
}
