use crate::store::persistence::fleet_repo::FleetPersist as _;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use dashmap::DashMap;
use tokio::io::AsyncWriteExt;
use tokio_util::task::TaskTracker;
use uuid::Uuid;

use crate::store::persistence::fleet::TreeCheckRow;
use crate::store::persistence::{PersistError, PersistHandle};

pub const RECORDING: &str = "recording result";
pub const BLOCKED: &str = "completion blocked";
const FIRST_BACKOFF: Duration = Duration::from_millis(100);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Captured {
    pub id: i64,
    pub exit_code: Option<i32>,
    pub output: String,
    pub state_after: String,
}

impl Captured {
    fn cancelled(&self) -> bool {
        self.state_after.starts_with("cancelled:")
    }
}

struct Pending {
    run: String,
    captured: Arc<Captured>,
    log: Result<PathBuf, String>,
    blocked: Option<String>,
}

fn pending() -> &'static DashMap<i64, Pending> {
    static PENDING: OnceLock<DashMap<i64, Pending>> = OnceLock::new();
    PENDING.get_or_init(DashMap::new)
}

pub fn phase(run: &str) -> Option<&'static str> {
    let mut found = None;
    for item in pending().iter().filter(|item| item.run == run) {
        if item.blocked.is_some() {
            return Some(BLOCKED);
        }
        found = Some(RECORDING);
    }
    found
}

pub fn blocked(run: &str) -> Result<(), String> {
    let why = pending()
        .iter()
        .filter(|item| item.run == run)
        .find_map(|item| item.blocked.clone());
    match why {
        Some(why) => Err(why),
        None => Ok(()),
    }
}

async fn save_output(repo: &Path, captured: &Captured) -> Result<PathBuf, String> {
    let directory = crate::launch::fleet::gate_slot::managed_directory(repo)?;
    let path = directory.join(format!(
        "mike-check-{}-{}-output.log",
        captured.id,
        Uuid::new_v4()
    ));
    write_output(&path, &captured.output)
        .await
        .map_err(|err| format!("{}: {err}", path.display()))?;
    Ok(path)
}

async fn write_output(path: &Path, output: &str) -> std::io::Result<()> {
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .await?;
    file.write_all(output.as_bytes()).await?;
    file.sync_all().await
}

pub async fn record(
    persist: &PersistHandle,
    tracker: &TaskTracker,
    run: &str,
    repo: &str,
    captured: Captured,
) -> Result<TreeCheckRow, String> {
    let captured = Arc::new(captured);
    let id = captured.id;
    pending().insert(
        id,
        Pending {
            run: run.to_string(),
            captured: captured.clone(),
            log: Err("output log is being written".into()),
            blocked: None,
        },
    );
    preserve_output(run, repo, &captured).await;
    let result = tokio::select! {
        result = retry(persist, run, &captured, |delay| pause(tracker, delay)) => result,
        _ = persist.pool().close_event() => Err(PersistError::Database(sqlx::Error::PoolClosed)),
    };
    match result {
        Ok(row) => {
            pending().remove(&id);
            Ok(row)
        }
        Err(err) if captured.cancelled() => {
            pending().remove(&id);
            tracing::warn!(%run, receipt = id, error = %err, "fleet: a cancelled receipt's result could not be recorded; later checks are not blocked by it");
            Err(format!(
                "receipt {id} was cancelled and its result could not be recorded: {err}"
            ))
        }
        Err(err) => Err(mark_blocked(id, &err)),
    }
}

async fn preserve_output(run: &str, repo: &str, captured: &Captured) {
    let log = save_output(Path::new(repo), captured).await;
    if let Err(err) = &log {
        tracing::error!(%run, error = %err, "fleet: could not preserve the check's output log");
    }
    if let Some(mut item) = pending().get_mut(&captured.id) {
        item.log = log;
    }
}

async fn pause(tracker: &TaskTracker, delay: Duration) -> Result<(), PersistError> {
    let wake = tokio::time::Instant::now() + delay;
    loop {
        if tracker.is_closed() {
            return Err(PersistError::DataInvariant(
                "proxy tasks are draining; check completion persistence has stopped".into(),
            ));
        }
        let now = tokio::time::Instant::now();
        if now >= wake {
            return Ok(());
        }
        tokio::time::sleep_until(wake.min(now + Duration::from_secs(1))).await;
    }
}

fn mark_blocked(id: i64, err: &PersistError) -> String {
    let Some(mut item) = pending().get_mut(&id) else {
        return format!("check completion blocked: {err}");
    };
    let run = item.run.clone();
    let output = match &item.log {
        Ok(path) => format!("Full captured output: {}", path.display()),
        Err(why) => format!("Output remains in memory only; its log could not be saved: {why}"),
    };
    let why = format!(
        "receipt {} command ended with exit {:?}, but its result could not be recorded: {err}. The command was not rerun. {output}. Automatic recovery has stopped; restart replay is not supported",
        item.captured.id, item.captured.exit_code
    );
    item.blocked = Some(why.clone());
    tracing::error!(%run, receipt = id, %why, "fleet: check completion blocked");
    why
}

trait CompletionStore {
    fn receipt(
        &self,
        run: &str,
        id: i64,
    ) -> impl Future<Output = Result<Option<TreeCheckRow>, PersistError>> + Send;

    fn finish(&self, captured: &Captured) -> impl Future<Output = Result<(), PersistError>> + Send;
}

impl CompletionStore for PersistHandle {
    async fn receipt(&self, _run: &str, id: i64) -> Result<Option<TreeCheckRow>, PersistError> {
        self.fleet().tree_check(id).await
    }

    async fn finish(&self, captured: &Captured) -> Result<(), PersistError> {
        self.fleet()
            .finish_tree_check(
                captured.id,
                captured.exit_code,
                &captured.output,
                Some(&captured.state_after),
            )
            .await
    }
}

async fn retry<S, P, F>(
    store: &S,
    run: &str,
    captured: &Captured,
    mut pause: P,
) -> Result<TreeCheckRow, PersistError>
where
    S: CompletionStore,
    P: FnMut(Duration) -> F,
    F: Future<Output = Result<(), PersistError>>,
{
    let mut backoff = FIRST_BACKOFF;
    let mut attempts = 0u64;
    loop {
        match attempt(store, run, captured).await {
            Ok(row) => return Ok(row),
            Err(err) if transient(&err) => {
                attempts = attempts.saturating_add(1);
                tracing::warn!(%run, error = %err, attempts, ?backoff, "fleet: retrying check result persistence; command has ended");
                pause(backoff).await?;
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }
            Err(err) => return Err(err),
        }
    }
}

async fn attempt(
    store: &impl CompletionStore,
    run: &str,
    captured: &Captured,
) -> Result<TreeCheckRow, PersistError> {
    let row = current(store, run, captured.id).await?;
    if row.finished_at.is_some() {
        return matching(row, captured);
    }
    store.finish(captured).await?;
    matching(current(store, run, captured.id).await?, captured)
}

async fn current(
    store: &impl CompletionStore,
    run: &str,
    id: i64,
) -> Result<TreeCheckRow, PersistError> {
    store
        .receipt(run, id)
        .await?
        .filter(|row| row.id == id && row.run_id == run)
        .ok_or_else(|| {
            PersistError::DataInvariant(format!("receipt {id} is not a receipt of run {run}"))
        })
}

fn matching(row: TreeCheckRow, captured: &Captured) -> Result<TreeCheckRow, PersistError> {
    if row.finished_at.is_none()
        || row.exit_code != captured.exit_code
        || row.output != captured.output
        || row.state_after.as_deref() != Some(captured.state_after.as_str())
    {
        return Err(PersistError::DataInvariant(format!(
            "receipt {} does not match the captured completion",
            captured.id
        )));
    }
    Ok(row)
}

fn transient(err: &PersistError) -> bool {
    let (PersistError::Database(err) | PersistError::Connect { source: err, .. }) = err else {
        return false;
    };
    match err {
        sqlx::Error::Io(_) | sqlx::Error::PoolTimedOut => true,
        sqlx::Error::Database(err) => err.code().is_some_and(|code| transient_code(&code)),
        _ => false,
    }
}

fn transient_code(code: &str) -> bool {
    code.starts_with("08")
        || matches!(
            code,
            "40001" | "40P01" | "55P03" | "53300" | "57014" | "57P01" | "57P02" | "57P03"
        )
}

#[cfg(test)]
#[path = "check_completion_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "check_completion_receipt_tests.rs"]
mod receipt_tests;
