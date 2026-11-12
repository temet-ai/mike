use std::future::Future;

use axum::http::StatusCode;
use tokio::time::Instant;

use crate::feature_toggles::client_serves;
use crate::state::AppState;

pub(crate) async fn by_deadline<T: Send + 'static>(
    deadline: Instant,
    work: impl Future<Output = T> + Send + 'static,
) -> Option<T> {
    tokio::time::timeout_at(deadline, work).await.ok()
}

pub(crate) fn require_client_serves(state: &AppState, key: &str) -> Result<(), StatusCode> {
    if client_serves(state, key) {
        Ok(())
    } else {
        Err(StatusCode::CONFLICT)
    }
}
