pub mod declared;
pub mod files;
pub mod frontmatter;
pub mod sync;

#[cfg(test)]
pub mod declared_tests;
#[cfg(test)]
pub mod files_tests;
#[cfg(test)]
pub mod frontmatter_tests;
#[cfg(test)]
pub mod sync_db_tests;

use std::path::PathBuf;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::state::AppState;
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

pub fn spawn_knowledge_scan(state: &AppState) {
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
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => return,
            _ = interval.tick() => {
                tick(&persist, &claude_home, &mut memo, &mut legacy_merged).await;
            }
        }
    }
}

async fn tick(
    persist: &PersistHandle,
    claude_home: &std::path::Path,
    memo: &mut ScanMemo,
    legacy_merged: &mut bool,
) {
    let Some(identity) = load_machine().await else {
        return;
    };
    if !*legacy_merged {
        *legacy_merged = crate::machine_identity::merge_legacy_at_startup(persist, &identity).await;
    }
    match scan_machine(persist, identity.machine_uuid, claude_home, memo).await {
        Ok(report) => log_report(&report),
        Err(error) => tracing::warn!(%error, "knowledge_scan: cycle failed"),
    }
    record_origins(persist, identity.machine_uuid).await;
}

async fn record_origins(persist: &PersistHandle, machine: uuid::Uuid) {
    if let Err(error) = crate::workspace::checkouts::record_origins(persist, machine).await {
        tracing::warn!(%error, "knowledge_scan: recording checkout remotes failed");
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
