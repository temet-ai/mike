use std::future::Future;

use tokio::task::JoinHandle;
use tokio_util::task::TaskTracker;
use tracing::Instrument;

pub fn spawn_tracked<F>(tracker: &TaskTracker, task: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tracker.spawn(task.in_current_span())
}

pub fn spawn_detached<F>(task: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(task.in_current_span())
}

#[cfg(test)]
#[path = "spans_tests.rs"]
pub mod spans_tests;
