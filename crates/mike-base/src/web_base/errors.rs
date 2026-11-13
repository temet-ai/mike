//! Responses every inspect handler returns when persistence is off or fails.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

pub fn persist_unavailable_response() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        "session persistence is disabled; pin/ttl endpoints are not available",
    )
        .into_response()
}

pub fn persist_error_response(e: crate::store::persistence::PersistError) -> Response {
    tracing::warn!(error = %e, "persistence operation failed in pin/ttl handler");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "persistence write failed",
    )
        .into_response()
}
