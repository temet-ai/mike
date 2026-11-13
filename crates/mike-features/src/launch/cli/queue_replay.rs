use std::path::{Path, PathBuf};

use super::ledger::Ledger;
use super::queue::{LocalQueue, QueuedClose, QueuedLaunch};

pub const STUCK_AFTER: u32 = 3;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Replay {
    pub replayed: usize,
    pub stuck: Vec<(PathBuf, String)>,
}

fn remove(path: &Path) {
    if let Err(e) = std::fs::remove_file(path) {
        eprintln!(
            "mike spawn: could not remove {} after replay: {e}",
            path.display()
        );
    }
}

async fn follow_ups(ledger: &Ledger, id: i64, queued: &QueuedLaunch) -> Result<(), String> {
    if let Some(refs) = &queued.opened {
        ledger.try_record_opened(id, refs).await?;
    }
    if let Some(outcome) = &queued.outcome {
        ledger.try_record_outcome(id, outcome).await?;
    }
    if let Some(closed) = &queued.closed {
        ledger.try_record_closed(id, closed).await?;
    }
    Ok(())
}

fn retry_later<T: serde::Serialize>(
    queue: &LocalQueue,
    path: &Path,
    value: &T,
    attempts: u32,
    why: &str,
    replay: &mut Replay,
) {
    if let Err(e) = queue.save_named(path, value) {
        eprintln!(
            "mike spawn: could not keep {} for a later replay: {e}",
            path.display()
        );
    }
    if attempts >= STUCK_AFTER {
        replay.stuck.push((
            path.to_path_buf(),
            format!("{} after {attempts} attempts: {why}", path.display()),
        ));
    }
}

async fn replay_launch(
    ledger: &Ledger,
    queue: &LocalQueue,
    path: &Path,
    mut queued: QueuedLaunch,
    replay: &mut Replay,
) {
    match ledger.try_insert_launch(&queued.launch).await {
        Ok(id) => {
            if let Err(e) = follow_ups(ledger, id, &queued).await {
                eprintln!("mike spawn: launch {id} replayed, but a later update was not: {e}");
            }
            remove(path);
            replay.replayed += 1;
        }
        Err(why) => {
            queued.attempts += 1;
            let attempts = queued.attempts;
            retry_later(queue, path, &queued, attempts, &why, replay);
        }
    }
}

async fn replay_close(
    ledger: &Ledger,
    queue: &LocalQueue,
    path: &Path,
    mut queued: QueuedClose,
    replay: &mut Replay,
) {
    let result = match ledger.try_find_launch_by_session(queued.session).await {
        Ok(Some(row)) => ledger.try_record_closed(row.id, &queued.closed).await,
        Ok(None) => {
            eprintln!(
                "mike spawn: no launch is recorded for {}; its queued close is dropped",
                queued.session
            );
            Ok(())
        }
        Err(e) => Err(e),
    };
    match result {
        Ok(()) => {
            remove(path);
            replay.replayed += 1;
        }
        Err(why) => {
            queued.attempts += 1;
            let attempts = queued.attempts;
            retry_later(queue, path, &queued, attempts, &why, replay);
        }
    }
}

pub async fn replay(ledger: &Ledger) -> Replay {
    let mut replay = Replay::default();
    let Some(queue) = ledger.queue() else {
        return replay;
    };
    if !ledger.available() {
        return replay;
    }
    for (path, run) in queue.runs() {
        if ledger.try_upsert_run(&run).await.is_ok() {
            remove(&path);
            replay.replayed += 1;
        }
    }
    for (path, queued) in queue.launches() {
        replay_launch(ledger, queue, &path, queued, &mut replay).await;
    }
    for (path, queued) in queue.closes() {
        replay_close(ledger, queue, &path, queued, &mut replay).await;
    }
    if replay.replayed > 0 {
        eprintln!(
            "mike spawn: replayed {} queued ledger record(s) to {}",
            replay.replayed,
            ledger.route()
        );
    }
    replay
}
