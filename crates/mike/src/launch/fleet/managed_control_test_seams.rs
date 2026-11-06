use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::launch::fleet::{build_admission, build_admission_root, owned_execution, owned_git};

pub use owned_execution::{Drain, ExecutionState, Phase};

#[derive(Default)]
pub struct Registry(owned_execution::Registry);
pub struct Fence(owned_execution::RunFence);
pub struct Ticket(owned_execution::Registration);
#[derive(Clone)]
pub struct Owner(owned_execution::Owner);
pub struct Permit(build_admission::Permit);

impl Registry {
    pub async fn fence(&self, run: &str) -> Fence {
        Fence(self.0.fence(run).await)
    }
    pub fn snapshot(&self, run: &str) -> Drain {
        self.0.snapshot(run)
    }
    pub async fn drain(&self, run: &str, patience: Duration) -> Drain {
        self.0.drain(run, patience).await
    }
}

impl Fence {
    pub fn register(&self, ledger: &str) -> Result<Ticket, String> {
        self.0.register(ledger).map(Ticket)
    }
    pub fn stopping_persisted(&self) {
        self.0.stopping_persisted();
    }
    pub fn establish_known_empty(&self) -> Result<(), String> {
        self.0.establish_known_empty()
    }
}

impl Ticket {
    pub fn owner(&self) -> Owner {
        Owner(self.0.owner())
    }
    pub fn receipt_recorded(self) {
        self.0.receipt_recorded();
    }
    pub fn receipt_was_not_created(self) {
        self.0.receipt_was_not_created();
    }
    pub fn receipt_blocked(self, why: String) {
        self.0.receipt_blocked(why);
    }
}

impl Owner {
    pub fn token(&self) -> &CancellationToken {
        self.0.token()
    }
    pub fn receipt(&self, receipt: i64) {
        self.0.receipt(receipt);
    }
    pub fn phase(&self, phase: Phase, why: Option<String>) {
        self.0.phase(phase, why);
    }
    pub fn cancel(&self, why: impl Into<String>) {
        self.0.cancel(why);
    }
    pub fn hold_permit(&self, permit: Permit) -> Result<(), String> {
        self.0.hold_permit(permit.0)
    }
    pub fn release_cleaned_permit(&self) {
        self.0.release_cleaned_permit();
    }
    pub fn process_started(&self) {
        self.0.process_started();
    }
    pub fn process_finished(&self) {
        self.0.process_finished();
    }
}

pub async fn acquire(root: &Path, cancellation: &CancellationToken) -> Result<Permit, String> {
    build_admission::acquire(root, cancellation)
        .await
        .map(Permit)
        .map_err(|error| error.to_string())
}

pub async fn acquire_resource(
    root: &Path,
    resource: &str,
    cancellation: &CancellationToken,
) -> Result<Permit, String> {
    crate::launch::fleet::operation_admission::acquire(root, &[resource.to_string()], cancellation)
        .await
        .map(Permit)
}

pub fn prepare_root(state: &Path) -> Result<PathBuf, String> {
    build_admission_root::prepare_in(state)
}

pub async fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    owned_git::run(repo, args).await
}

pub async fn git_with_limit(
    repo: &Path,
    args: &[&str],
    limit: Duration,
    cancellation: CancellationToken,
) -> Result<String, String> {
    owned_git::run_fixture(repo, args, limit, cancellation).await
}

pub fn has_managed_context() -> bool {
    crate::launch::fleet::managed_command::current().is_ok()
}

pub async fn hold_checkout(
    owner: &Owner,
    repo: &Path,
    patience: Duration,
) -> Result<tokio::sync::OwnedMutexGuard<()>, String> {
    crate::launch::fleet::managed_command::hold_checkout_owned(&owner.0, repo, patience).await
}

pub async fn raw_checkout_hold(repo: &Path) -> tokio::sync::OwnedMutexGuard<()> {
    crate::launch::fleet::gate_slot::queue::hold(repo).await
}

pub fn normalize_cancellation(owner: &Owner, code: &mut Option<i32>, output: &mut String) -> bool {
    crate::launch::fleet::managed_command::normalize_cancellation(&owner.0, code, output)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn stat_group(text: &str) -> Result<(char, libc::pid_t), String> {
    crate::launch::fleet::managed_process::group::census::stat_group(text)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn ps_quiet(text: &str, group: libc::pid_t) -> Result<bool, String> {
    crate::launch::fleet::managed_process::group::census::ps_quiet(text, group)
}

impl Owner {
    pub fn process_started_holding(&self, resource: std::sync::Arc<()>) {
        self.0.process_started_with(vec![resource]);
    }
}
