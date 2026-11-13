#[cfg(test)]
use std::future::Future;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio_util::task::TaskTracker;
use uuid::Uuid;

#[cfg(test)]
use super::definition::FleetDefinition;
#[cfg(test)]
use super::gate::Target;
use super::managed_command::{self, Context};
use super::owned_execution::{Registration, registry};
use super::owned_spawn_fence::{Allowed, Authority};

#[path = "managed_fixture_authority.rs"]
pub mod authority;
pub use authority::{FixtureAuthority, FixtureFence};
#[cfg(test)]
#[path = "managed_durable_test_support.rs"]
pub mod durable;
#[cfg(test)]
pub use durable::{durable_gate, running_fleet, startup_gate};

#[derive(Clone)]
pub struct PrivateRoot {
    guard: Arc<tempfile::TempDir>,
    canonical: PathBuf,
}

tokio::task_local! {
    static PRIVATE_ROOT: PrivateRoot;
}

impl PrivateRoot {
    pub fn new() -> anyhow::Result<Self> {
        let guard = Arc::new(tempfile::tempdir()?);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(guard.path(), std::fs::Permissions::from_mode(0o700))?;
        }
        let canonical = std::fs::canonicalize(guard.path())?;
        Ok(Self { guard, canonical })
    }

    #[cfg(test)]
    pub async fn scope<T>(&self, work: impl Future<Output = T>) -> T {
        PRIVATE_ROOT.scope(self.clone(), work).await
    }
}

pub fn current_root_guard() -> Option<Arc<tempfile::TempDir>> {
    PRIVATE_ROOT
        .try_with(|root| root.guard.clone())
        .ok()
        .or_else(|| {
            managed_command::current()
                .ok()
                .and_then(|context| context.root_guard)
        })
}

pub fn current_root_override() -> Option<PathBuf> {
    PRIVATE_ROOT
        .try_with(|root| root.canonical.clone())
        .ok()
        .or_else(|| {
            managed_command::current()
                .ok()
                .and_then(|context| context.root_guard.map(|_| context.private_host_root))
        })
}

#[cfg(test)]
pub async fn with_private_root<T>(work: impl Future<Output = T>) -> anyhow::Result<T> {
    Ok(PrivateRoot::new()?.scope(work).await)
}

pub struct Execution {
    context: Context,
    registration: Registration,
    exact_run: String,
}

impl Execution {
    #[cfg(test)]
    pub async fn new(ledger: &str) -> anyhow::Result<Self> {
        Self::with_root(ledger, PrivateRoot::new()?).await
    }

    pub async fn with_root(ledger: &str, root: PrivateRoot) -> anyhow::Result<Self> {
        let exact_run = format!("fixture-{}", Uuid::new_v4());
        let local = registry().fence(&exact_run).await;
        let registration = local.register(ledger).map_err(anyhow::Error::msg)?;
        let owner = registration.owner();
        owner.admission_root(&root.canonical);
        let authority = FixtureAuthority::new(root.guard.clone(), &exact_run, Allowed::Running);
        let context = Context {
            queued_at: std::time::Instant::now(),
            receipt_id: None,
            owner,
            authority: Authority::Fixture(authority),
            private_host_root: root.canonical,
            tracker: TaskTracker::new(),
            ledger: ledger.to_string(),
            root_guard: Some(root.guard),
            resources: Default::default(),
            held_resources: Vec::new(),
            operation: None,
            strip_environment: Vec::new(),
        };
        Ok(Self {
            context,
            registration,
            exact_run,
        })
    }

    pub fn context(&self) -> Context {
        self.context.clone()
    }

    #[cfg(test)]
    pub async fn scope<T>(&self, work: impl Future<Output = T>) -> T {
        managed_command::scope(self.context(), work).await
    }

    pub fn root(&self) -> &std::path::Path {
        &self.context.private_host_root
    }

    pub fn phase(&self) -> Option<String> {
        registry()
            .snapshot(&self.exact_run)
            .pending
            .first()
            .map(|entry| format!("{:?}", entry.phase))
    }

    #[cfg(feature = "test-seams")]
    pub fn diagnostic(&self) -> Option<String> {
        registry()
            .snapshot(&self.exact_run)
            .pending
            .first()
            .and_then(|entry| entry.detail.clone())
    }

    pub async fn stop(&self) -> anyhow::Result<()> {
        let fence = registry().fence(&self.exact_run).await;
        let Authority::Fixture(authority) = &self.context.authority else {
            anyhow::bail!("fixture stop cannot modify durable fleet state");
        };
        authority
            .transition("stopping", Some(&self.exact_run))
            .await;
        fence.stopping_persisted();
        Ok(())
    }

    pub async fn finish(self) -> anyhow::Result<()> {
        self.context.tracker.close();
        tokio::time::timeout(Duration::from_secs(10), self.context.tracker.wait()).await?;
        self.registration.receipt_recorded();
        anyhow::ensure!(
            registry().snapshot(&self.exact_run).pending.is_empty(),
            "fixture owned-process cleanup remains unresolved"
        );
        Ok(())
    }
}

#[cfg(test)]
pub async fn with_context<T>(
    ledger: &str,
    work: impl Future<Output = T>,
) -> anyhow::Result<T> {
    let execution = Execution::new(ledger).await?;
    let output = execution.scope(work).await;
    execution.finish().await?;
    Ok(output)
}

#[cfg(test)]
pub async fn check(
    def: &FleetDefinition,
    at: &Target,
    run: &str,
) -> anyhow::Result<(Option<i32>, String)> {
    with_context(run, super::check_run::execute(def, at, run)).await
}

#[cfg(test)]
pub async fn gate(
    def: &FleetDefinition,
    at: &Target,
    run: &str,
) -> anyhow::Result<(Option<i32>, String, bool)> {
    with_context(run, super::gate::execute_verified(def, at, run)).await
}

#[cfg(test)]
pub async fn gate_command(
    slot: &Path,
    command: &str,
    run: &str,
) -> anyhow::Result<(Option<i32>, String)> {
    with_context(run, super::gate_slot::run_gate(slot, command, run)).await
}

pub async fn fixture_lifecycle_fence() -> anyhow::Result<()> {
    authority::fixture_lifecycle_change_waits_for_retained_spawn_fence().await
}

pub async fn fixture_rejects_wrong_identity() -> anyhow::Result<()> {
    authority::fixture_rejects_a_different_exact_run_and_a_wrong_lifecycle().await
}
