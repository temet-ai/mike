pub mod declared;
pub mod files;
pub mod frontmatter;
pub mod machine;
pub mod sync;

#[cfg(test)]
pub mod declared_tests;
#[cfg(test)]
pub mod files_tests;
#[cfg(test)]
pub mod frontmatter_tests;
#[cfg(test)]
pub mod machine_db_tests;
#[cfg(test)]
pub mod machine_tests;
#[cfg(test)]
pub mod scan_test_support;
#[cfg(test)]
pub mod sync_db_tests;
#[cfg(test)]
mod sync_restart_db_tests;

use std::path::PathBuf;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::semantic::KnowledgeState;
use crate::store::persistence::PersistHandle;

pub use sync::{ScanMemo, ScanReport, scan_machine};

const TICK: Duration = Duration::from_secs(120);

pub fn claude_home() -> Option<PathBuf> {
    if let Some(configured) = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(configured));
    }
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(|home| PathBuf::from(home).join(".claude"))
}

pub fn spawn_knowledge_scan(state: &KnowledgeState) {
    let Some(persist) = state.persist().cloned() else {
        tracing::debug!("knowledge_scan: no persistence, not spawning");
        return;
    };
    let Some(claude_home) = claude_home() else {
        tracing::warn!("knowledge_scan: neither CLAUDE_CONFIG_DIR nor HOME is set, not spawning");
        return;
    };
    let shutdown = state.shutdown();
    state.tracker.spawn(async move {
        run(persist, claude_home, shutdown).await;
    });
}

async fn run(persist: PersistHandle, claude_home: PathBuf, shutdown: CancellationToken) {
    let mut interval = tokio::time::interval(TICK);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut memo = ScanMemo::default();
    let mut legacy_merged = false;
    let mut machine_recorded = false;
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => return,
            _ = interval.tick() => {
                let once = OnceSteps { legacy_merged: &mut legacy_merged, machine_recorded: &mut machine_recorded };
                tick(&persist, &claude_home, &mut memo, once).await;
            }
        }
    }
}

/// Steps a scan does once per process: each flag turns true when its step succeeds.
/// The machine row is never removed while this machine runs, so recording it on
/// every tick only repeated an insert that did nothing.
struct OnceSteps<'a> {
    legacy_merged: &'a mut bool,
    machine_recorded: &'a mut bool,
}

async fn tick(
    persist: &PersistHandle,
    claude_home: &std::path::Path,
    memo: &mut ScanMemo,
    once: OnceSteps<'_>,
) {
    let Some(identity) = load_machine().await else {
        return;
    };
    once_steps(persist, &identity, once).await;
    match scan_machine(persist, identity.machine_uuid, claude_home, memo).await {
        Ok(report) => log_report(&report),
        Err(error) => tracing::warn!(%error, "knowledge_scan: cycle failed"),
    }
    record_origins(persist, identity.machine_uuid).await;
}

async fn once_steps(
    persist: &PersistHandle,
    identity: &crate::machine_identity::MachineIdentity,
    once: OnceSteps<'_>,
) {
    if !*once.legacy_merged {
        *once.legacy_merged =
            crate::machine_identity::merge_legacy_at_startup(persist, identity).await;
    }
    if !*once.machine_recorded {
        *once.machine_recorded = record_machine(persist, identity).await;
    }
}

async fn record_machine(
    persist: &PersistHandle,
    identity: &crate::machine_identity::MachineIdentity,
) -> bool {
    match persist
        .ensure_machine(identity.machine_uuid, &identity.machine_name)
        .await
    {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(%error, "knowledge_scan: recording this machine failed");
            false
        }
    }
}

async fn record_origins(persist: &PersistHandle, machine: uuid::Uuid) {
    match crate::workspace::checkouts::record_origins(persist, machine).await {
        Ok(mut moves) => finish_moves(persist, &mut moves).await,
        Err(error) => tracing::warn!(%error, "knowledge_scan: recording checkout remotes failed"),
    }
}

async fn finish_moves(
    persist: &PersistHandle,
    moves: &mut [crate::store::persistence::location_move::LocationMove],
) {
    use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
    if let Err(error) = persist.knowledge().finish_location_moves(moves).await {
        tracing::warn!(%error, "knowledge_scan: rewriting moved knowledge source roots failed");
    }
}

fn log_report(report: &ScanReport) {
    if !report.changed() {
        return;
    }
    tracing::info!(
        scanned = report.scanned,
        updated = report.updated,
        failed = report.failed,
        "knowledge_scan: cycle finished"
    );
}

async fn load_machine() -> Option<crate::machine_identity::MachineIdentity> {
    match crate::machine_identity::load_default().await {
        Ok(identity) => Some(identity),
        Err(error) => {
            tracing::warn!(%error, "knowledge_scan: no machine identity, skipping this cycle");
            None
        }
    }
}
