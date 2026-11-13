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

pub fn knowledge_error(error: crate::store::persistence::PersistError) -> Response {
    match error {
        crate::store::persistence::PersistError::DataInvariant(message) => {
            tracing::warn!(error = %message, "project knowledge request refused as a conflict");
            (
                StatusCode::CONFLICT,
                axum::Json(serde_json::json!({"error": message})),
            )
                .into_response()
        }
        other => persist_error_response(other),
    }
}
