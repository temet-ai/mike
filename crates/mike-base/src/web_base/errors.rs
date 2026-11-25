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
    if let Some(response) = temporary_response(&e) {
        return response;
    }
    tracing::warn!(error = %e, "persistence operation failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "persistence operation failed",
    )
        .into_response()
}

fn temporarily_unavailable(e: &crate::store::persistence::PersistError, reason: &str) -> Response {
    let pressure = super::db_pressure::describe();
    tracing::warn!(error = %e, reason, pressure, "database temporarily unavailable; request refused with 503");
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [(axum::http::header::RETRY_AFTER, "5")],
        axum::Json(serde_json::json!({
            "error": format!("database temporarily unavailable: {reason}")
        })),
    )
        .into_response()
}

pub fn temporary_response(e: &crate::store::persistence::PersistError) -> Option<Response> {
    match crate::store::persistence::capture_retry::classify(e) {
        crate::store::persistence::capture_retry::WriteFailure::Temporary(reason) => {
            Some(temporarily_unavailable(e, &reason))
        }
        _ => None,
    }
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

#[cfg(test)]
#[path = "errors_tests.rs"]
mod tests;
