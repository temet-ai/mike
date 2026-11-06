use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use dashmap::DashMap;
use tokio::sync::{OwnedMutexGuard, watch};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub fn managed_containment_capability() -> &'static str {
    if cfg!(target_os = "macos") {
        "owned-group/macos-source-reviewed-unvalidated"
    } else if cfg!(target_os = "linux") {
        "owned-group/linux-procfs"
    } else {
        "owned-group/unsupported"
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Registering,
    WaitingCheckout,
    WaitingAdmission,
    Preparing,
    Executing,
    Cleaning,
    RecordingReceipt,
    ReceiptBlocked,
    CleanupBlocked,
    Interrupted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionState {
    pub containment: &'static str,
    pub id: Uuid,
    pub receipt: Option<i64>,
    pub ledger: String,
    pub phase: Phase,
    pub detail: Option<String>,
    pub admission_root: Option<PathBuf>,
    pub owned_processes: usize,
}

#[derive(Clone, Debug)]
pub struct Drain {
    pub known_in_this_process: bool,
    pub stop_requested: bool,
    pub pending: Vec<ExecutionState>,
}

impl Drain {
    pub fn locally_settled(&self) -> bool {
        self.known_in_this_process && self.stop_requested && self.pending.is_empty()
    }

    fn blocked(&self) -> bool {
        self.pending.iter().any(|entry| {
            matches!(
                entry.phase,
                Phase::ReceiptBlocked | Phase::CleanupBlocked | Phase::Interrupted
            )
        })
    }
}

fn never_ran(entry: &ExecutionState) -> bool {
    entry.phase == Phase::ReceiptBlocked && entry.owned_processes == 0
}

struct Run {
    gate: Arc<tokio::sync::Mutex<()>>,
    cancellation: CancellationToken,
    entries: Mutex<BTreeMap<Uuid, ExecutionState>>,
    changed: watch::Sender<u64>,
    observed: AtomicBool,
    permits: Mutex<BTreeMap<Uuid, super::build_admission::Permit>>,
    resources: Mutex<BTreeMap<Uuid, Vec<super::managed_resources::Resource>>>,
}

impl Run {
    fn new() -> Self {
        Self {
            gate: Arc::new(tokio::sync::Mutex::new(())),
            cancellation: CancellationToken::new(),
            entries: Mutex::new(BTreeMap::new()),
            changed: watch::channel(0).0,
            observed: AtomicBool::new(false),
            permits: Mutex::new(BTreeMap::new()),
            resources: Mutex::new(BTreeMap::new()),
        }
    }

    fn change(&self, action: impl FnOnce(&mut BTreeMap<Uuid, ExecutionState>)) {
        action(
            &mut self
                .entries
                .lock()
                .unwrap_or_else(|error| error.into_inner()),
        );
        self.changed
            .send_modify(|version| *version = version.wrapping_add(1));
    }

    fn drain(&self) -> Drain {
        Drain {
            known_in_this_process: self.observed.load(Ordering::Acquire),
            stop_requested: self.cancellation.is_cancelled(),
            pending: self
                .entries
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .values()
                .cloned()
                .collect(),
        }
    }
}

#[derive(Default)]
pub(crate) struct Registry {
    runs: DashMap<String, Arc<Run>>,
}

pub(crate) fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(Registry::default)
}

pub(crate) struct RunFence {
    run: Arc<Run>,
    _held: OwnedMutexGuard<()>,
}

impl Registry {
    pub(crate) async fn fence(&self, exact_run: &str) -> RunFence {
        let run = self
            .runs
            .entry(exact_run.to_string())
            .or_insert_with(|| Arc::new(Run::new()))
            .clone();
        let held = run.gate.clone().lock_owned().await;
        RunFence { run, _held: held }
    }

    pub(super) fn snapshot(&self, exact_run: &str) -> Drain {
        self.runs.get(exact_run).map_or_else(
            || Drain {
                known_in_this_process: false,
                stop_requested: false,
                pending: Vec::new(),
            },
            |run| run.drain(),
        )
    }

    pub(super) fn unrun_receipts(&self, exact_run: &str) -> Vec<ExecutionState> {
        let Some(run) = self.runs.get(exact_run).map(|run| run.clone()) else {
            return Vec::new();
        };
        let permitted: Vec<Uuid> = run
            .permits
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .keys()
            .copied()
            .collect();
        run.entries
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .values()
            .filter(|entry| never_ran(entry) && !permitted.contains(&entry.id))
            .cloned()
            .collect()
    }

    pub(super) fn forget_unrun(&self, exact_run: &str, id: Uuid) {
        if let Some(run) = self.runs.get(exact_run).map(|run| run.clone()) {
            run.change(|entries| {
                if entries.get(&id).is_some_and(never_ran) {
                    entries.remove(&id);
                }
            });
        }
    }

    pub(super) async fn drain(&self, exact_run: &str, patience: Duration) -> Drain {
        let Some(run) = self.runs.get(exact_run).map(|run| run.clone()) else {
            return self.snapshot(exact_run);
        };
        let mut changed = run.changed.subscribe();
        let waiting = async {
            loop {
                let snapshot = run.drain();
                if snapshot.locally_settled() || snapshot.blocked() {
                    return snapshot;
                }
                if changed.changed().await.is_err() {
                    return run.drain();
                }
            }
        };
        tokio::time::timeout(patience, waiting)
            .await
            .unwrap_or_else(|_| run.drain())
    }
}

impl RunFence {
    pub(crate) fn establish_known_empty(&self) -> Result<(), String> {
        if !self
            .run
            .entries
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .is_empty()
        {
            return Err("managed executions remain; empty-run reconciliation was refused".into());
        }
        self.run.observed.store(true, Ordering::Release);
        self.run
            .changed
            .send_modify(|version| *version = version.wrapping_add(1));
        Ok(())
    }

    pub(crate) fn is_stopping(&self) -> bool {
        self.run.cancellation.is_cancelled()
    }

    pub(super) fn stopping_persisted(&self) {
        self.run.cancellation.cancel();
        self.run
            .changed
            .send_modify(|version| *version = version.wrapping_add(1));
    }

    pub(super) fn register(&self, ledger: &str) -> Result<Registration, String> {
        if self.run.cancellation.is_cancelled() {
            return Err("managed command refused: this run is stopping".into());
        }
        self.run.observed.store(true, Ordering::Release);
        let id = Uuid::new_v4();
        let state = ExecutionState {
            containment: managed_containment_capability(),
            id,
            receipt: None,
            ledger: ledger.into(),
            phase: Phase::Registering,
            detail: None,
            admission_root: None,
            owned_processes: 0,
        };
        self.run.change(|entries| {
            entries.insert(id, state);
        });
        Ok(Registration {
            owner: Owner {
                id,
                run: self.run.clone(),
                cancellation: self.run.cancellation.child_token(),
                reason: Arc::new(Mutex::new(None)),
            },
            settled: false,
        })
    }
}

#[derive(Clone)]
pub(super) struct Owner {
    id: Uuid,
    run: Arc<Run>,
    cancellation: CancellationToken,
    reason: Arc<Mutex<Option<String>>>,
}

impl Owner {
    #[cfg(any(test, feature = "test-seams"))]
    pub(super) fn process_started(&self) {
        self.process_started_with(Vec::new());
    }

    pub(super) fn process_started_with(&self, held: Vec<super::managed_resources::Resource>) {
        let mut resources = self
            .run
            .resources
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        resources.entry(self.id).or_default().extend(held);
        self.run.change(|entries| {
            if let Some(entry) = entries.get_mut(&self.id) {
                entry.owned_processes = entry.owned_processes.saturating_add(1);
            }
        });
    }

    pub(super) fn process_finished(&self) {
        let mut resources = self
            .run
            .resources
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let mut quiet = false;
        self.run.change(|entries| {
            if let Some(entry) = entries.get_mut(&self.id) {
                entry.owned_processes = entry.owned_processes.saturating_sub(1);
                quiet = entry.owned_processes == 0;
            }
        });
        if quiet {
            resources.remove(&self.id);
        }
    }

    fn cleanup_pending(&self) -> bool {
        let processes = self
            .run
            .entries
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(&self.id)
            .is_some_and(|entry| entry.owned_processes != 0);
        processes
            || self
                .run
                .permits
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .contains_key(&self.id)
    }

    pub(super) fn admission_root(&self, root: &Path) {
        self.run.change(|entries| {
            if let Some(entry) = entries.get_mut(&self.id) {
                entry.admission_root = Some(root.to_path_buf());
            }
        });
    }

    pub(super) fn hold_permit(&self, permit: super::build_admission::Permit) -> Result<(), String> {
        let mut permits = self
            .run
            .permits
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if permits.contains_key(&self.id) {
            return Err("execution already owns a managed build permit".into());
        }
        permits.insert(self.id, permit);
        Ok(())
    }

    pub(super) fn release_cleaned_permit(&self) {
        self.run
            .permits
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.id);
    }

    pub(super) fn token(&self) -> &CancellationToken {
        &self.cancellation
    }

    pub(super) async fn fence(&self) -> OwnedMutexGuard<()> {
        self.run.gate.clone().lock_owned().await
    }

    pub(super) fn cancel(&self, reason: impl Into<String>) {
        let mut recorded = self
            .reason
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        recorded.get_or_insert_with(|| reason.into());
        self.cancellation.cancel();
    }

    pub(super) fn cancelled(&self) -> Option<String> {
        self.cancellation.is_cancelled().then(|| {
            self.reason
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clone()
                .unwrap_or_else(|| "fleet stop requested".into())
        })
    }

    pub(super) fn phase(&self, phase: Phase, detail: Option<String>) {
        self.run.change(|entries| {
            if let Some(entry) = entries.get_mut(&self.id)
                && !matches!(
                    entry.phase,
                    Phase::Interrupted | Phase::ReceiptBlocked | Phase::CleanupBlocked
                )
            {
                entry.phase = phase;
                entry.detail = detail;
            }
        });
    }

    pub(super) fn receipt(&self, receipt: i64) {
        self.run.change(|entries| {
            if let Some(entry) = entries.get_mut(&self.id) {
                entry.receipt = Some(receipt);
            }
        });
    }
}

pub(super) struct Registration {
    owner: Owner,
    settled: bool,
}

impl Registration {
    pub(super) fn owner(&self) -> Owner {
        self.owner.clone()
    }

    pub(super) fn receipt_recorded(mut self) {
        if self.owner.cleanup_pending() {
            self.owner.phase(
                Phase::CleanupBlocked,
                Some("receipt recorded before owned process cleanup released admission".into()),
            );
            self.settled = true;
            return;
        }
        self.owner.run.change(|entries| {
            entries.remove(&self.owner.id);
        });
        self.settled = true;
    }

    pub(super) fn receipt_was_not_created(self) {
        self.receipt_recorded();
    }

    pub(super) fn receipt_blocked(mut self, why: String) {
        self.owner.phase(Phase::ReceiptBlocked, Some(why));
        self.settled = true;
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        if !self.settled {
            self.owner
                .cancel("execution task ended before a terminal receipt was verified");
            self.owner.phase(
                Phase::Interrupted,
                Some("receipt and process cleanup require reconciliation".into()),
            );
        }
    }
}

#[cfg(test)]
#[path = "owned_execution_tests.rs"]
mod tests;
