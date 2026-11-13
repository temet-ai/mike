use std::time::Duration;

use tokio::time::{MissedTickBehavior, interval};
use tokio_util::sync::CancellationToken;

use crate::store::persistence::PersistHandle;
use crate::workspace::boot_id::CURRENT_BOOT_ID;

pub fn spawn_workspace_reaper(
    persist: Option<PersistHandle>,
    shutdown: CancellationToken,
    interval_secs: u64,
    tracker: &tokio_util::task::TaskTracker,
) {
    let Some(persist) = persist else {
        log_not_spawning_no_persistence();
        return;
    };
    let Some(boot_id) = CURRENT_BOOT_ID.clone() else {
        log_not_spawning_no_boot_id();
        return;
    };

    let interval_secs = interval_secs.max(1);

    log_spawned(interval_secs, &boot_id);

    tracker.spawn(run_reaper_loop(persist, boot_id, shutdown, interval_secs));
}

fn log_not_spawning_no_persistence() {
    tracing::debug!("workspace_reaper: no persistence — not spawning");
}

fn log_not_spawning_no_boot_id() {
    tracing::info!(
        "workspace_reaper: current boot id could not be determined on this platform — not spawning"
    );
}

fn log_spawned(interval_secs: u64, boot_id: &str) {
    tracing::info!(interval_secs, boot_id = %boot_id, "workspace_reaper: spawned");
}

async fn run_reaper_loop(
    persist: PersistHandle,
    boot_id: String,
    shutdown: CancellationToken,
    interval_secs: u64,
) {
    let mut ticker = interval(Duration::from_secs(interval_secs));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            biased;

            _ = shutdown.cancelled() => {
                tracing::debug!("workspace_reaper: shutdown signalled, exiting");
                break;
            }
            _ = ticker.tick() => {
                sweep_once(&persist, &boot_id).await;
            }
        }
    }
}

const LAPSED_LEASE_GRACE: chrono::Duration = chrono::Duration::hours(24);

pub async fn sweep_once(persist: &PersistHandle, boot_id: &str) {
    let mut dead = dead_candidate_ids(persist, boot_id).await;
    dead.extend(lapsed_registration_ids(persist).await);
    dead.sort_unstable();
    dead.dedup();
    if dead.is_empty() {
        return;
    }
    reap_and_log(persist, &dead).await;
}

async fn lapsed_registration_ids(persist: &PersistHandle) -> Vec<i64> {
    let lapsed_before = chrono::Utc::now() - LAPSED_LEASE_GRACE;
    match persist.fetch_lapsed_registration_ids(lapsed_before).await {
        Ok(ids) => ids,
        Err(e) => {
            tracing::warn!(error = %e, "workspace_reaper: fetch_lapsed_registration_ids failed");
            Vec::new()
        }
    }
}

async fn dead_candidate_ids(persist: &PersistHandle, boot_id: &str) -> Vec<i64> {
    let candidates = match persist.fetch_reap_candidates(boot_id).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "workspace_reaper: fetch_reap_candidates failed");
            return Vec::new();
        }
    };
    candidates
        .into_iter()
        .filter(|c| !is_process_alive(c.pid))
        .map(|c| c.session_id)
        .collect()
}

async fn reap_and_log(persist: &PersistHandle, dead: &[i64]) {
    match persist.reap_dead_registrations(dead).await {
        Ok(outcome) if outcome.reaped > 0 => log_reaped(outcome.reaped),
        Ok(_) => {}
        Err(e) => log_reap_failed(&e),
    }
}

fn log_reaped(reaped: i64) {
    tracing::info!(
        reaped,
        "workspace_reaper: reaped dead-process registrations"
    );
}

fn log_reap_failed(e: &crate::store::persistence::PersistError) {
    tracing::warn!(error = %e, "workspace_reaper: reap_dead_registrations failed");
}

#[cfg(unix)]
pub fn is_process_alive(pid: i32) -> bool {
    let Some(pid) = rustix::process::Pid::from_raw(pid) else {
        return false;
    };
    !matches!(
        rustix::process::test_kill_process(pid),
        Err(rustix::io::Errno::SRCH)
    )
}

#[cfg(not(unix))]
pub fn is_process_alive(_pid: i32) -> bool {
    true
}

#[cfg(all(test, unix))]
mod tests {
    use super::is_process_alive;

    #[test]
    fn is_process_alive_true_for_own_pid() {
        assert!(is_process_alive(std::process::id() as i32));
    }

    #[test]
    fn is_process_alive_false_for_an_unused_pid() {
        assert!(!is_process_alive(i32::MAX - 1));
    }
}
