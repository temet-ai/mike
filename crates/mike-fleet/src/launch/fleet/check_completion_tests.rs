use std::collections::VecDeque;
use std::sync::Mutex;

use chrono::Utc;

use super::*;

struct Store {
    reads: Mutex<VecDeque<Result<Option<TreeCheckRow>, PersistError>>>,
    finishes: Mutex<VecDeque<Result<(), PersistError>>>,
    captured: Mutex<Vec<Captured>>,
}

impl Store {
    fn scripted(
        reads: Vec<Result<Option<TreeCheckRow>, PersistError>>,
        finishes: Vec<Result<(), PersistError>>,
    ) -> Self {
        Self {
            reads: Mutex::new(reads.into()),
            finishes: Mutex::new(finishes.into()),
            captured: Mutex::new(Vec::new()),
        }
    }
}

fn locked<T>(value: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    value.lock().unwrap_or_else(|poison| poison.into_inner())
}

impl CompletionStore for Store {
    async fn latest(&self, _run: &str) -> Result<Option<TreeCheckRow>, PersistError> {
        locked(&self.reads)
            .pop_front()
            .unwrap_or_else(|| Err(PersistError::DataInvariant("no scripted read".into())))
    }

    async fn finish(&self, captured: &Captured) -> Result<(), PersistError> {
        locked(&self.captured).push(captured.clone());
        locked(&self.finishes).pop_front().unwrap_or(Ok(()))
    }
}

fn outcome() -> Captured {
    Captured {
        id: 123,
        exit_code: Some(7),
        output: "exact output\nsecond line\0\n".into(),
        state_after: "state".into(),
    }
}

fn row(finished: bool) -> TreeCheckRow {
    let captured = outcome();
    TreeCheckRow {
        id: captured.id,
        run_id: "run".into(),
        state: captured.state_after.clone(),
        state_after: finished.then_some(captured.state_after),
        command: "command executed once".into(),
        started_at: Utc::now(),
        finished_at: finished.then(Utc::now),
        exit_code: finished.then_some(captured.exit_code).flatten(),
        output: if finished {
            captured.output
        } else {
            String::new()
        },
        askers: vec!["seat".into()],
        told: Vec::new(),
    }
}

fn timeout() -> PersistError {
    PersistError::Database(sqlx::Error::PoolTimedOut)
}

#[tokio::test]
async fn failed_finish_and_failed_read_retry_only_the_same_captured_outcome() -> anyhow::Result<()>
{
    let completed = row(true);
    let store = Store::scripted(
        vec![
            Ok(Some(row(false))),
            Err(timeout()),
            Ok(Some(row(false))),
            Ok(Some(completed.clone())),
        ],
        vec![Err(timeout()), Ok(())],
    );
    let mut delays = Vec::new();
    let recovered = retry(&store, "run", &outcome(), |delay| {
        delays.push(delay);
        std::future::ready(Ok(()))
    })
    .await?;
    assert_eq!(recovered, completed);
    assert_eq!(*locked(&store.captured), vec![outcome(), outcome()]);
    assert_eq!(delays, vec![FIRST_BACKOFF, FIRST_BACKOFF * 2]);
    Ok(())
}

#[tokio::test]
async fn failed_read_after_successful_finish_does_not_finish_again() -> anyhow::Result<()> {
    let completed = row(true);
    let store = Store::scripted(
        vec![
            Ok(Some(row(false))),
            Err(timeout()),
            Ok(Some(completed.clone())),
        ],
        vec![Ok(())],
    );
    let recovered = retry(&store, "run", &outcome(), |_| std::future::ready(Ok(()))).await?;
    assert_eq!(recovered, completed);
    assert_eq!(*locked(&store.captured), vec![outcome()]);
    Ok(())
}

#[tokio::test]
async fn lost_finish_acknowledgement_reads_committed_result_without_rewriting_it()
-> anyhow::Result<()> {
    let completed = row(true);
    let store = Store::scripted(
        vec![Ok(Some(row(false))), Ok(Some(completed.clone()))],
        vec![Err(timeout())],
    );
    let recovered = retry(&store, "run", &outcome(), |_| std::future::ready(Ok(()))).await?;
    assert_eq!(recovered, completed);
    assert_eq!(*locked(&store.captured), vec![outcome()]);
    Ok(())
}

#[tokio::test]
async fn closed_pool_is_terminal_and_never_sleeps_or_retries() {
    let store = Store::scripted(
        vec![Err(PersistError::Database(sqlx::Error::PoolClosed))],
        Vec::new(),
    );
    let result = retry(&store, "run", &outcome(), |_| async {
        panic!("terminal error retried")
    })
    .await;
    assert!(matches!(
        result,
        Err(PersistError::Database(sqlx::Error::PoolClosed))
    ));
    assert!(locked(&store.captured).is_empty());
}

#[tokio::test]
async fn backoff_is_capped_and_eventual_recovery_keeps_the_original_result() -> anyhow::Result<()> {
    let completed = row(true);
    let mut reads = (0..15).map(|_| Err(timeout())).collect::<Vec<_>>();
    reads.push(Ok(Some(completed.clone())));
    let store = Store::scripted(reads, Vec::new());
    let mut delays = Vec::new();
    let recovered = retry(&store, "run", &outcome(), |delay| {
        delays.push(delay);
        std::future::ready(Ok(()))
    })
    .await?;
    assert_eq!(recovered, completed);
    assert_eq!(delays.len(), 15);
    assert_eq!(delays.last(), Some(&MAX_BACKOFF));
    assert!(delays.iter().all(|delay| *delay <= MAX_BACKOFF));
    assert!(locked(&store.captured).is_empty());
    Ok(())
}

#[tokio::test]
async fn missing_receipt_or_conflicting_completion_stops_without_overwriting() {
    let mut conflicting = row(true);
    conflicting.output = "different result".into();
    let mut wrong_id = row(true);
    wrong_id.id += 1;
    let mut wrong_run = row(true);
    wrong_run.run_id = "other".into();
    for latest in [None, Some(conflicting), Some(wrong_id), Some(wrong_run)] {
        let store = Store::scripted(vec![Ok(latest)], Vec::new());
        let result = retry(&store, "run", &outcome(), |_| async {
            panic!("invariant error retried")
        })
        .await;
        assert!(matches!(result, Err(PersistError::DataInvariant(_))));
        assert!(locked(&store.captured).is_empty());
    }
}

#[test]
fn retry_classification_excludes_closed_pool_schema_permissions_and_constraints() {
    assert!(transient(&timeout()));
    assert!(!transient(&PersistError::Database(sqlx::Error::PoolClosed)));
    assert!(!transient(&PersistError::DataInvariant(
        "wrong receipt".into()
    )));
    for code in [
        "08006", "40001", "40P01", "55P03", "53300", "57014", "57P01", "57P03",
    ] {
        assert!(transient_code(code), "{code}");
    }
    for code in ["28P01", "42501", "42P01", "42703", "23505", "22021"] {
        assert!(!transient_code(code), "{code}");
    }
}

#[tokio::test]
async fn execution_logs_are_unique_and_preserve_every_output_byte() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let captured = outcome();
    let first = save_output(root.path(), &captured)
        .await
        .map_err(anyhow::Error::msg)?;
    let second = save_output(root.path(), &captured)
        .await
        .map_err(anyhow::Error::msg)?;
    assert_ne!(first, second);
    assert_eq!(tokio::fs::read(&first).await?, captured.output.as_bytes());
    assert_eq!(tokio::fs::read(&second).await?, captured.output.as_bytes());
    assert!(write_output(&first, "replacement").await.is_err());
    assert_eq!(tokio::fs::read(&first).await?, captured.output.as_bytes());
    Ok(())
}

#[tokio::test]
async fn unsavable_output_log_reports_failure() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    std::fs::write(root.path().join(".claude"), "not a directory")?;
    let result = save_output(root.path(), &outcome()).await;
    assert!(result.is_err());
    Ok(())
}

#[test]
fn terminal_status_retains_outcome_and_names_its_output_location() {
    let run = Uuid::new_v4().to_string();
    assert_eq!(phase(&run), None);
    assert!(blocked(&run).is_ok());
    pending().insert(
        run.clone(),
        Pending {
            captured: Arc::new(outcome()),
            log: Ok(PathBuf::from("/project/execution.log")),
            blocked: None,
        },
    );
    assert_eq!(phase(&run), Some(RECORDING));
    let why = mark_blocked(&run, &PersistError::Database(sqlx::Error::PoolClosed));
    assert!(why.contains("receipt 123"));
    assert!(why.contains("/project/execution.log"));
    assert!(why.contains("restart replay is not supported"));
    assert_eq!(phase(&run), Some(BLOCKED));
    assert_eq!(blocked(&run), Err(why));
    let retained = pending().remove(&run).map(|(_, value)| value.captured);
    assert_eq!(retained.as_deref(), Some(&outcome()));
}

#[tokio::test]
async fn retry_pause_stops_when_proxy_tasks_are_draining() {
    let tracker = TaskTracker::new();
    tracker.close();
    let result = pause(&tracker, MAX_BACKOFF).await;
    assert!(matches!(result, Err(PersistError::DataInvariant(_))));
}

#[tokio::test]
async fn retry_pause_returns_after_its_backoff_when_proxy_is_active() -> anyhow::Result<()> {
    pause(&TaskTracker::new(), Duration::ZERO).await?;
    Ok(())
}

#[tokio::test]
#[cfg(unix)]
async fn output_preservation_refuses_symlinked_managed_directories() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    std::fs::create_dir(root.path().join(".claude"))?;
    std::os::unix::fs::symlink(outside.path(), root.path().join(".claude/worktrees"))?;
    let result = save_output(root.path(), &outcome()).await;
    assert!(result.is_err_and(|why| why.contains("ordinary directory")));
    assert_eq!(std::fs::read_dir(outside.path())?.count(), 0);
    Ok(())
}
