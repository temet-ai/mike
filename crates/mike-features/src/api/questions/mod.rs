pub mod answer;
pub mod ask;
pub mod mail;
pub mod resolve;
pub mod text;

#[cfg(test)]
pub mod tests;

pub use answer::{post_question_answer, post_question_decline};
pub use ask::post_question;

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

pub fn problem(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": message.into() }))).into_response()
}

pub fn unavailable() -> Response {
    problem(
        StatusCode::SERVICE_UNAVAILABLE,
        "session persistence is disabled; questions are not available",
    )
}

pub fn database(err: impl std::fmt::Display) -> Response {
    tracing::warn!(error = %err, "questions: database error");
    problem(StatusCode::INTERNAL_SERVER_ERROR, "database error")
}
