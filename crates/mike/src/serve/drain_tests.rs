use std::time::Duration;

use tokio_util::task::TaskTracker;

use super::{DrainOutcome, drain_tracker};

#[tokio::test]
async fn drain_tracker_with_stuck_task_reports_timeout_and_error() -> anyhow::Result<()> {
    let tracker = TaskTracker::new();
    tracker.spawn(std::future::pending::<()>());
    let limit = Duration::from_secs(1);

    let outcome = drain_tracker(tracker, limit).await;

    assert_eq!(
        outcome,
        DrainOutcome::TimedOut {
            unfinished_tasks: 1
        }
    );
    let Err(error) = outcome.into_result(limit) else {
        anyhow::bail!("a timed-out drain must be an error so the process exits non-zero");
    };
    assert!(error.to_string().contains("timed out after 1s"), "{error}");
    assert!(error.to_string().contains("1 background task"), "{error}");
    Ok(())
}

#[tokio::test]
async fn drain_tracker_with_finished_tasks_completes_ok() -> anyhow::Result<()> {
    let tracker = TaskTracker::new();
    tracker.spawn(async {
        tokio::task::yield_now().await;
    });
    let limit = Duration::from_secs(30);

    let outcome = drain_tracker(tracker, limit).await;

    assert_eq!(outcome, DrainOutcome::Completed);
    outcome.into_result(limit)?;
    Ok(())
}
