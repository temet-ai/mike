mod answer;
mod ask;
mod mail;
mod resolve;
mod text;

#[cfg(test)]
mod tests;

pub(crate) use answer::{post_question_answer, post_question_decline};
pub(crate) use ask::post_question;

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

pub(super) fn problem(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": message.into() }))).into_response()
}

pub(super) fn unavailable() -> Response {
    problem(
        StatusCode::SERVICE_UNAVAILABLE,
        "session persistence is disabled; questions are not available",
    )
}

pub(super) fn database(err: impl std::fmt::Display) -> Response {
    tracing::warn!(%err, "questions: database error");
    problem(StatusCode::INTERNAL_SERVER_ERROR, "database error")
}
