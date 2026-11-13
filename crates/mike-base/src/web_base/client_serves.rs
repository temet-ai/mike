use std::future::Future;

use axum::http::StatusCode;
use tokio::time::Instant;

use crate::base_state::BaseState;

pub fn client_serves(state: &BaseState, key: &str) -> bool {
    crate::feature_toggles::get_toggle_enabled(&state.feature_toggle_overlay.load(), key)
        .unwrap_or(false)
}

pub async fn by_deadline<T: Send + 'static>(
    deadline: Instant,
    work: impl Future<Output = T> + Send + 'static,
) -> Option<T> {
    tokio::time::timeout_at(deadline, work).await.ok()
}

pub fn require_client_serves(state: &BaseState, key: &str) -> Result<(), StatusCode> {
    if client_serves(state, key) {
        Ok(())
    } else {
        Err(StatusCode::CONFLICT)
    }
}
