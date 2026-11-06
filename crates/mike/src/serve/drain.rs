use std::time::Duration;

use tokio_util::task::TaskTracker;

pub(super) const TRACKER_DRAIN_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DrainOutcome {
    Completed,
    TimedOut { unfinished_tasks: usize },
}

pub(super) async fn drain_tracker(tracker: TaskTracker, limit: Duration) -> DrainOutcome {
    tracker.close();
    match tokio::time::timeout(limit, tracker.wait()).await {
        Ok(()) => DrainOutcome::Completed,
        Err(_) => DrainOutcome::TimedOut {
            unfinished_tasks: tracker.len(),
        },
    }
}

impl DrainOutcome {
    pub(super) fn into_result(self, limit: Duration) -> anyhow::Result<()> {
        match self {
            DrainOutcome::Completed => Ok(()),
            DrainOutcome::TimedOut { unfinished_tasks } => {
                let message = format!(
                    "shutdown drain timed out after {}s with {unfinished_tasks} background task(s) \
                     unfinished; captures they were still writing may be lost",
                    limit.as_secs()
                );
                tracing::error!("{message}");
                Err(anyhow::anyhow!(message))
            }
        }
    }
}

#[cfg(test)]
#[path = "drain_tests.rs"]
mod drain_tests;
