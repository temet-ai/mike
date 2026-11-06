pub(crate) mod installs;
pub(crate) mod reads;
pub(crate) mod state;
pub(crate) mod writes;

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": message.into() }))).into_response()
}

fn unavailable() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "session persistence is disabled; learned skills are not available",
    )
}

fn persist_failed(e: &crate::store::persistence::PersistError) -> Response {
    error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
