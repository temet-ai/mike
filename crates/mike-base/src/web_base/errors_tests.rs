use axum::http::StatusCode;

use super::persist_error_response;
use crate::store::persistence::PersistError;

#[test]
fn persist_error_response_pool_timeout_is_503_with_retry_after() {
    let response = persist_error_response(PersistError::Database(sqlx::Error::PoolTimedOut));

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::RETRY_AFTER)
            .map(|v| v.as_bytes()),
        Some(&b"5"[..])
    );
}

#[test]
fn persist_error_response_lasting_failure_is_500() {
    let response = persist_error_response(PersistError::DataInvariant("broken row".to_string()));

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
