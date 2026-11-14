use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::sync::OwnedMutexGuard;
use tokio::task::JoinHandle;

use super::super::managed_command;
use super::super::managed_process;
use super::super::managed_test_support::{self, PrivateRoot};

pub struct Domain {
    root: PrivateRoot,
}

impl Domain {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            root: PrivateRoot::new()?,
        })
    }

    pub async fn execution(&self, ledger: &str) -> anyhow::Result<Execution> {
        Ok(Execution {
            inner: managed_test_support::Execution::with_root(ledger, self.root.clone()).await?,
        })
    }
}

pub struct Execution {
    inner: managed_test_support::Execution,
}

pub struct Command {
    pub cwd: PathBuf,
    pub text: String,
    pub timeout: Duration,
    pub log: Option<PathBuf>,
}

impl Command {
    fn into_managed(self, ledger: &str) -> managed_process::Command {
        managed_process::Command {
            slot: self.cwd,
            text: self.text,
            run: ledger.into(),
            budget: (self.timeout, "fixture command".into()),
            log: self.log,
            setting: None,
        }
    }
}

impl Execution {
    pub fn spawn(&self, command: Command) -> JoinHandle<(Option<i32>, String)> {
        self.spawn_inner(command, false)
    }

    pub fn spawn_queued(&self, command: Command) -> JoinHandle<(Option<i32>, String)> {
        self.spawn_inner(command, true)
    }

    pub fn spawn_declaring(
        &self,
        command: Command,
        resource: &str,
    ) -> JoinHandle<(Option<i32>, String)> {
        let mut context = self.inner.context();
        context.operation = Some(super::durable::declared(resource));
        let command = command.into_managed(&context.ledger);
        tokio::spawn(managed_command::scope(
            context,
            managed_process::run(command),
        ))
    }

    fn spawn_inner(&self, command: Command, queued: bool) -> JoinHandle<(Option<i32>, String)> {
        let context = self.inner.context();
        let command = command.into_managed(&context.ledger);
        tokio::spawn(managed_command::scope(context, async move {
            let _held = if queued {
                match managed_command::hold_checkout(&command.slot, Some(command.budget.0)).await {
                    Ok(held) => Some(held),
                    Err(why) => return (None, why),
                }
            } else {
                None
            };
            managed_process::run(command).await
        }))
    }

    pub fn cancel(&self, reason: &str) {
        self.inner.context().owner.cancel(reason);
    }

    pub async fn stop(&self) -> anyhow::Result<()> {
        self.inner.stop().await
    }

    pub fn phase(&self) -> Option<String> {
        self.inner.phase()
    }

    pub fn diagnostic(&self) -> Option<String> {
        self.inner.diagnostic()
    }

    pub fn root(&self) -> &Path {
        self.inner.root()
    }

    pub async fn finish(self) -> anyhow::Result<()> {
        self.inner.finish().await
    }
}

pub async fn hold_checkout(path: &Path) -> OwnedMutexGuard<()> {
    super::super::gate_slot::queue::hold(path).await
}

pub async fn without_context(command: Command) -> (Option<i32>, String) {
    managed_process::run(command.into_managed("missing-context")).await
}

pub async fn fixture_lifecycle_fence() -> anyhow::Result<()> {
    managed_test_support::fixture_lifecycle_fence().await
}

pub async fn fixture_rejects_wrong_identity() -> anyhow::Result<()> {
    managed_test_support::fixture_rejects_wrong_identity().await
}

impl Execution {
    pub fn spawn_git_queued(
        &self,
        repo: PathBuf,
        args: Vec<String>,
        file: std::fs::File,
    ) -> JoinHandle<Result<String, String>> {
        let context = self.inner.context();
        tokio::spawn(managed_command::scope(context, async move {
            let _checkout =
                managed_command::hold_checkout(&repo, Some(Duration::from_secs(10))).await?;
            let context = managed_command::current()?;
            let _file = context.resources.retain(file);
            let args = args.iter().map(String::as_str).collect::<Vec<_>>();
            super::super::owned_git::run(&repo, &args).await
        }))
    }
}
