use std::future::Future;

use axum::http::Request;
use tokio::task::JoinHandle;
use tokio_util::task::TaskTracker;
use tracing::{Instrument, Span};

pub(crate) fn spawn_tracked<F>(tracker: &TaskTracker, task: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tracker.spawn(task.in_current_span())
}

pub(crate) fn spawn_detached<F>(task: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(task.in_current_span())
}

pub(crate) fn inspect_request_span<B>(request: &Request<B>) -> Span {
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    tracing::info_span!(
        "inspect_request",
        method = %request.method(),
        path = %request.uri().path(),
        request_id,
    )
}

#[cfg(test)]
#[path = "spans_tests.rs"]
mod spans_tests;
