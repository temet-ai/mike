use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio_util::task::TaskTracker;
use uuid::Uuid;

use super::super::managed_command::{self, Context};
use super::super::managed_process;
use super::super::owned_execution::{self, Drain, Phase};
use super::super::owned_spawn_fence::{Allowed, Identity, SpawnFence};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::TreeCheckRow;

pub struct Domain(Arc<tempfile::TempDir>);

pub fn declared(resource: &str) -> super::super::operation::Operation {
    super::super::operation::Operation {
        resources: vec![resource.into()],
        ..Default::default()
    }
}

pub struct Request {
    pub fleet: Uuid,
    pub run: String,
    pub ledger: String,
    pub command: String,
}

pub struct Queued {
    queued: managed_command::Queued,
    persist: PersistHandle,
    run: String,
    command: String,
}

pub struct Command {
    pub cwd: PathBuf,
    pub timeout: Duration,
    pub log: Option<PathBuf>,
}

impl Domain {
    pub fn new() -> anyhow::Result<Self> {
        let root = Arc::new(tempfile::tempdir()?);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self(root))
    }

    pub fn path(&self) -> anyhow::Result<PathBuf> {
        Ok(self.0.path().canonicalize()?)
    }

    pub async fn begin(&self, persist: PersistHandle, request: Request) -> Result<Queued, String> {
        let mut queued = managed_command::begin(
            identity(persist.clone(), request.fleet, &request.run),
            managed_command::Receipt {
                ledger: &request.ledger,
                state: "durable-fixture-source",
                command: &request.command,
                seat: "fixture",
            },
            self.path().map_err(|error| error.to_string())?,
            TaskTracker::new(),
        )
        .await?;
        queued.context.root_guard = Some(self.0.clone());
        Ok(Queued {
            queued,
            persist,
            run: request.run,
            command: request.command,
        })
    }
}

impl Queued {
    pub fn declaring(mut self, resource: &str) -> Self {
        self.queued.context.operation = Some(declared(resource));
        self
    }

    pub fn with_receipt_writer(mut self, persist: PersistHandle) -> Self {
        self.persist = persist;
        self
    }

    pub fn id(&self) -> i64 {
        self.queued.id
    }

    pub fn spawn(self, command: Command) -> JoinHandle<Result<TreeCheckRow, String>> {
        tokio::spawn(async move {
            let context = self.queued.context.clone();
            let process = managed_process::Command {
                slot: command.cwd.clone(),
                text: self.command.clone(),
                run: self.run.clone(),
                budget: (command.timeout, "durable fixture".into()),
                log: command.log,
                setting: None,
            };
            let result = managed_command::scope(context, managed_process::run(process)).await;
            self.record(command.cwd, result).await
        })
    }

    pub fn spawn_git(
        self,
        command: Command,
        args: Vec<String>,
    ) -> JoinHandle<Result<TreeCheckRow, String>> {
        tokio::spawn(async move {
            let context = self.queued.context.clone();
            let args = args.iter().map(String::as_str).collect::<Vec<_>>();
            let result =
                managed_command::scope(context, super::super::owned_git::run(&command.cwd, &args))
                    .await;
            let result = match result {
                Ok(output) => (Some(0), output),
                Err(why) => (None, why),
            };
            self.record(command.cwd, result).await
        })
    }

    pub async fn cancel_without_spawn(self, cwd: PathBuf) -> Result<TreeCheckRow, String> {
        self.queued
            .context
            .owner
            .cancel("fixture queued command cancelled before execution");
        self.record(cwd, (None, "never launched".into())).await
    }

    async fn record(
        self,
        cwd: PathBuf,
        (mut code, mut output): (Option<i32>, String),
    ) -> Result<TreeCheckRow, String> {
        let Context {
            owner,
            tracker,
            ledger,
            ..
        } = &self.queued.context;
        let cancelled = managed_command::normalize_cancellation(owner, &mut code, &mut output);
        owner.phase(Phase::RecordingReceipt, None);
        let captured = super::super::check_run::completion::Captured {
            id: self.queued.id,
            exit_code: code,
            output,
            state_after: if cancelled {
                "cancelled:unverified"
            } else {
                "unverified-source"
            }
            .into(),
        };
        let row = super::super::check_run::completion::record(
            &self.persist,
            tracker,
            ledger,
            &cwd.to_string_lossy(),
            captured,
        )
        .await;
        tracker.close();
        tracker.wait().await;
        match row {
            Ok(row) => {
                self.queued.registration.receipt_recorded();
                Ok(row)
            }
            Err(why) => {
                self.queued.registration.receipt_blocked(why.clone());
                Err(why)
            }
        }
    }
}

fn identity(persist: PersistHandle, fleet: Uuid, run: &str) -> Identity {
    Identity {
        persist,
        fleet,
        run: run.into(),
        allowed: Allowed::Running,
    }
}

pub struct RetainedSpawnFence(SpawnFence);

pub async fn retain_spawn_fence(
    persist: PersistHandle,
    fleet: Uuid,
    run: &str,
) -> Result<RetainedSpawnFence, String> {
    identity(persist, fleet, run)
        .lock()
        .await
        .map(RetainedSpawnFence)
}

impl RetainedSpawnFence {
    pub async fn commit(self) -> Result<(), String> {
        self.0.commit().await
    }
}

pub struct LocalStopFence(owned_execution::RunFence);

pub async fn local_stop_fence(run: &str) -> LocalStopFence {
    LocalStopFence(owned_execution::registry().fence(run).await)
}

impl LocalStopFence {
    pub fn stopping_persisted(&self) {
        self.0.stopping_persisted();
    }
}

pub fn snapshot(run: &str) -> Drain {
    owned_execution::registry().snapshot(run)
}

pub async fn leave_blocked_receipt(
    run: &str,
    ledger: &str,
    receipt: Option<i64>,
    why: &str,
) -> Result<(), String> {
    let fence = owned_execution::registry().fence(run).await;
    let registration = fence.register(ledger)?;
    if let Some(receipt) = receipt {
        registration.owner().receipt(receipt);
    }
    registration.receipt_blocked(why.to_string());
    Ok(())
}
