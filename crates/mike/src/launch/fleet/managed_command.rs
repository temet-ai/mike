use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::sync::OwnedMutexGuard;
use tokio_util::task::TaskTracker;

use super::build_admission::Permit;
use super::managed_queue::Lane;
pub(super) use super::managed_resources::{Resource, Resources};
use super::owned_execution::{Owner, Phase, Registration, registry};
use super::owned_spawn_fence::{Activity, Authority, Identity};

#[derive(Clone)]
pub(super) struct Context {
    pub queued_at: std::time::Instant,
    pub receipt_id: Option<i64>,
    pub owner: Owner,
    pub authority: Authority,
    #[cfg(any(test, feature = "test-seams"))]
    pub root_guard: Option<std::sync::Arc<tempfile::TempDir>>,
    pub private_host_root: PathBuf,
    pub tracker: TaskTracker,
    pub ledger: String,
    pub resources: Resources,
    pub held_resources: Vec<Resource>,
    pub operation: Option<super::operation::Operation>,
    pub strip_environment: Vec<String>,
}

pub(super) struct Receipt<'a> {
    pub ledger: &'a str,
    pub state: &'a str,
    pub command: &'a str,
    pub seat: &'a str,
}

pub(super) struct Queued {
    pub id: i64,
    pub registration: Registration,
    pub context: Context,
}

tokio::task_local! {
    static CURRENT: Context;
}

pub(super) async fn begin(
    identity: Identity,
    receipt: Receipt<'_>,
    private_host_root: PathBuf,
    tracker: TaskTracker,
) -> Result<Queued, String> {
    let local = registry().fence(&identity.run).await;
    let mut durable = identity.lock().await?;
    let registration = local.register(receipt.ledger)?;
    let owner = registration.owner();
    owner.admission_root(&private_host_root);
    let insert = sqlx::query_scalar::<_, i64>(
        "INSERT INTO launch.tree_checks (run_id, state, command, askers) VALUES ($1, $2, $3, ARRAY[$4::text]) RETURNING id",
    )
    .bind(receipt.ledger).bind(receipt.state).bind(receipt.command).bind(receipt.seat)
    .fetch_one(durable.connection());
    let inserted = tokio::time::timeout(Duration::from_secs(2), insert)
        .await
        .map_err(|_| "managed receipt insertion timed out before commit".to_string())
        .and_then(|result| result.map_err(|error| error.to_string()));
    let id = match inserted {
        Ok(id) => id,
        Err(error) => {
            registration.receipt_was_not_created();
            return Err(format!(
                "managed receipt transaction failed before commit: {error}"
            ));
        }
    };
    owner.receipt(id);
    if let Err(error) = durable.commit().await {
        registration.receipt_blocked(format!("queue transaction commit outcome unknown: {error}"));
        return Err(error);
    }
    owner.phase(Phase::WaitingCheckout, None);
    Ok(Queued {
        id,
        registration,
        context: Context {
            queued_at: std::time::Instant::now(),
            receipt_id: Some(id),
            owner,
            authority: Authority::Durable(identity),
            #[cfg(any(test, feature = "test-seams"))]
            root_guard: super::managed_test_support::current_root_guard(),
            private_host_root,
            tracker,
            ledger: receipt.ledger.into(),
            resources: Resources::default(),
            held_resources: Vec::new(),
            operation: None,
            strip_environment: Vec::new(),
        },
    })
}

pub(super) fn current() -> Result<Context, String> {
    CURRENT
        .try_with(|current| {
            let mut context = current.clone();
            context.held_resources.extend(context.resources.snapshot());
            context
        })
        .map_err(|_| "managed execution context is missing; command was not launched".into())
}

pub(super) async fn scope<T>(context: Context, work: impl Future<Output = T>) -> T {
    let receipt = context.receipt_id;
    let done = scoped(context, work).await;
    super::managed_queue::leave(receipt);
    done
}

async fn scoped<T>(context: Context, work: impl Future<Output = T>) -> T {
    let monitor = context.clone();
    CURRENT
        .scope(context, async move {
            tokio::pin!(work);
            tokio::select! {
                result = &mut work => result,
                _ = monitor_state(&monitor) => work.await,
            }
        })
        .await
}

async fn monitor_state(context: &Context) {
    let mut unverified = Unverified::default();
    loop {
        tokio::select! {
            biased;
            _ = context.owner.token().cancelled() => return,
            _ = tokio::time::sleep(STATE_POLL) => {}
        }
        let activity = tokio::time::timeout(STATE_READ_PATIENCE, context.authority.activity())
            .await
            .unwrap_or_else(|_| Activity::Unknown("the fleet state read timed out".into()));
        if let Some(why) = unverified.observe(activity, std::time::Instant::now()) {
            context
                .owner
                .cancel(format!("managed execution cancelled: {why}"));
            return;
        }
    }
}

const STATE_POLL: Duration = Duration::from_secs(2);
const STATE_READ_PATIENCE: Duration = Duration::from_secs(10);
const UNVERIFIED_LIMIT: Duration = Duration::from_secs(300);

#[derive(Default)]
pub(super) struct Unverified {
    since: Option<std::time::Instant>,
}

impl Unverified {
    pub(super) fn observe(
        &mut self,
        activity: Activity,
        now: std::time::Instant,
    ) -> Option<String> {
        match activity {
            Activity::Active => {
                self.since = None;
                None
            }
            Activity::Ended(why) => Some(why),
            Activity::Unknown(why) => {
                let since = *self.since.get_or_insert(now);
                (now.duration_since(since) >= UNVERIFIED_LIMIT).then(|| {
                    format!(
                        "fleet state could not be verified for {} seconds: {why}",
                        UNVERIFIED_LIMIT.as_secs()
                    )
                })
            }
        }
    }
}

pub(super) async fn hold_checkout(
    repo: &Path,
    patience: Duration,
) -> Result<std::sync::Arc<OwnedMutexGuard<()>>, String> {
    let context = current()?;
    let held = hold_checkout_owned(&context.owner, repo, patience).await?;
    Ok(context.resources.retain(held))
}

pub(super) async fn hold_checkout_owned(
    owner: &Owner,
    repo: &Path,
    patience: Duration,
) -> Result<OwnedMutexGuard<()>, String> {
    owner.phase(Phase::WaitingCheckout, None);
    let waiting = super::gate_slot::queue::hold(repo);
    tokio::select! {
        biased;
        _ = owner.token().cancelled() => Err(cancelled(owner)),
        held = tokio::time::timeout(patience, waiting) => held.map_err(|_| "timed out waiting for the shared build slot".into()),
    }
}

pub(super) async fn admit(context: &Context) -> Result<Permit, String> {
    context.owner.phase(Phase::WaitingAdmission, None);
    let declared = context
        .operation
        .as_ref()
        .map_or(&[][..], |operation| operation.resources.as_slice());
    super::operation_admission::acquire(&context.private_host_root, declared, context.owner.token())
        .await
}

pub(super) fn cancelled(owner: &Owner) -> String {
    format!(
        "managed command cancelled: {}",
        owner
            .cancelled()
            .unwrap_or_else(|| "execution was interrupted".into())
    )
}

pub(super) fn private_host_root() -> Result<PathBuf, String> {
    #[cfg(any(test, feature = "test-seams"))]
    if let Some(root) = super::managed_test_support::current_root_override() {
        return Ok(root);
    }
    super::build_admission_root::prepare()
}

pub(super) async fn begin_for_runner(
    runner: &super::check::Runner<'_>,
    receipt: Receipt<'_>,
) -> Result<Queued, String> {
    let identity = Identity {
        persist: runner.persist.clone(),
        fleet: runner.fleet,
        run: super::check_run::base_run(runner.run).to_string(),
        allowed: super::owned_spawn_fence::Allowed::Running,
    };
    begin(
        identity,
        receipt,
        private_host_root().map_err(|why| format!("managed command was not queued: {why}"))?,
        runner.tracker.clone(),
    )
    .await
    .map_err(|why| format!("managed command was not queued: {why}"))
}

async fn admit_to(
    persist: &crate::store::persistence::PersistHandle,
    def: &super::definition::FleetDefinition,
    (lane, command, configured): (Lane, &str, Option<Duration>),
) -> Result<(), String> {
    let measured = super::managed_queue::measure(persist, command).await;
    let patience = configured.unwrap_or_else(|| super::managed_queue::patience_from(measured));
    super::managed_queue::admit(
        super::managed_queue::depth(&def.repo, lane),
        measured,
        patience,
    )
}

fn enlist(queued: &Queued, enlisted: &super::managed_queue::Enlisted<'_>) {
    super::managed_queue::enlist(queued.id, enlisted, queued.registration.owner());
}

pub(super) async fn begin_check(
    runner: &super::check::Runner<'_>,
    def: &super::definition::FleetDefinition,
    state: &str,
    seat: &str,
) -> Result<Queued, String> {
    let configured = def
        .execution
        .operation(&def.check.command)
        .and_then(|op| op.timeout_seconds)
        .map(Duration::from_secs);
    admit_to(
        runner.persist,
        def,
        (Lane::Check, &def.check.command, configured),
    )
    .await?;
    let mut queued = begin_for_runner(
        runner,
        Receipt {
            ledger: runner.run,
            state,
            command: &def.check.command,
            seat,
        },
    )
    .await?;
    configure(&mut queued.context, def, &def.check.command);
    enlist(
        &queued,
        &super::managed_queue::Enlisted {
            repo: &def.repo,
            lane: Lane::Check,
            ledger: runner.run,
            seat,
            state,
        },
    );
    Ok(queued)
}

pub(super) fn configure(
    context: &mut Context,
    def: &super::definition::FleetDefinition,
    command: &str,
) {
    context.operation = def.execution.operation(command).cloned();
    context
        .strip_environment
        .clone_from(&def.execution.strip_environment);
}

pub(super) async fn begin_gate(
    runner: &super::check::Runner<'_>,
    def: &super::definition::FleetDefinition,
    seat: &str,
    target: &super::gate::Target,
) -> Result<Queued, String> {
    admit_to(
        runner.persist,
        def,
        (
            Lane::Gate,
            &def.gate.command,
            Some(super::gate_slot::GATE_TIMEOUT),
        ),
    )
    .await?;
    let ledger = super::gate::ledger_for(def, runner.run);
    let mut queued = begin_for_runner(
        runner,
        Receipt {
            ledger: &ledger,
            state: &target.commit,
            command: &def.gate.command,
            seat,
        },
    )
    .await?;
    configure(&mut queued.context, def, &def.gate.command);
    enlist(
        &queued,
        &super::managed_queue::Enlisted {
            repo: &def.repo,
            lane: Lane::Gate,
            ledger: &ledger,
            seat,
            state: &target.commit,
        },
    );
    Ok(queued)
}

pub(super) async fn begin_baseline(
    persist: &crate::store::persistence::PersistHandle,
    def: &super::definition::FleetDefinition,
    facts: &super::plan::FleetFacts,
    commit: &str,
    tracker: &TaskTracker,
) -> Result<Queued, String> {
    let identity = Identity {
        persist: persist.clone(),
        fleet: facts
            .fleet
            .parse::<uuid::Uuid>()
            .map_err(|error| error.to_string())?,
        run: facts.run_id.clone(),
        allowed: super::owned_spawn_fence::Allowed::Starting,
    };
    let mut queued = begin(
        identity,
        Receipt {
            ledger: &super::gate::ledger_for(def, &facts.run_id),
            state: commit,
            command: &def.gate.command,
            seat: "startup-baseline",
        },
        private_host_root()?,
        tracker.clone(),
    )
    .await?;
    configure(&mut queued.context, def, &def.gate.command);
    Ok(queued)
}

pub(super) fn normalize_cancellation(
    owner: &Owner,
    code: &mut Option<i32>,
    output: &mut String,
) -> bool {
    let Some(reason) = owner.cancelled() else {
        return false;
    };
    *code = None;
    let reason = format!("managed command cancelled: {reason}");
    if !output.contains(&reason) {
        output.push_str(&format!("\n{reason}\n"));
    }
    true
}

#[cfg(test)]
#[path = "managed_command_tests.rs"]
mod tests;

pub(super) fn preparation_allowed() -> Result<(), String> {
    if let Ok(context) = current()
        && context.owner.token().is_cancelled()
    {
        return Err(cancelled(&context.owner));
    }
    Ok(())
}

pub(super) fn spawn_refused(owner: &Owner, why: String) -> String {
    let cause = if why.ends_with("fleet state or exact run no longer admits execution") {
        "fleet state or exact run changed before launch"
    } else {
        "spawn verification unavailable"
    };
    owner.cancel(format!("managed execution cancelled: {cause}: {why}"));
    cancelled(owner)
}
