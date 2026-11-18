use crate::store::persistence::web_repo::WebPersist as _;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::feature_toggles::FeatureToggles;
use crate::state::AppState;
use crate::store::persistence::{PersistError, PersistHandle};

pub(crate) mod attempt;
pub(crate) mod batch;
pub(crate) mod batch_blame;
pub(crate) mod batch_git;
pub(crate) mod batch_land;
pub(crate) mod batch_mail;
pub(crate) mod batch_rounds;
pub(crate) mod check;
pub(crate) mod engineer;
pub(crate) mod feedback;
pub(crate) mod follow;
pub(crate) mod land;
pub(crate) mod perf;
pub(crate) mod policy;
pub(crate) mod port;
pub(crate) mod prepare;
pub(crate) mod rounds;
pub(crate) mod runner;
pub(crate) mod worktree;

#[cfg(test)]
mod batch_refusal_tests;
#[cfg(test)]
mod batch_tests;
#[cfg(test)]
mod fixtures_tests;
#[cfg(test)]
mod interrupted_tests;
#[cfg(test)]
mod landing_tests;
#[cfg(test)]
mod perf_tests;
#[cfg(test)]
mod policy_tests;
#[cfg(test)]
mod refusal_tests;
#[cfg(test)]
mod scratch_tests;
#[cfg(test)]
mod states_tests;

use attempt::AttemptEnd;
use batch_land::BatchEnd;
use engineer::InvestigateEngineer;
use port::{LiveBuild, RepairEngineer, RunningBinary};
use runner::{CommandRunner, ProcessRunner};
pub(crate) use worktree::RepoLayout;

pub(crate) mod states {
    pub(crate) const EDITING: &str = "editing";
    pub(crate) const PREPARED: &str = "prepared";
    pub(crate) const GATING: &str = "gating";
    pub(crate) const BATCHED: &str = "batched";
    pub(crate) const DROPPED: &str = "dropped";
    pub(crate) const GATE_FAILED: &str = "gate_failed";
    pub(crate) const REFUSED: &str = "refused";
    pub(crate) const LANDED: &str = "landed";
    pub(crate) const DEPLOYED: &str = "deployed";
    pub(crate) const HEALTHY: &str = "healthy";
    pub(crate) const ALL: [&str; 10] = [
        EDITING,
        PREPARED,
        GATING,
        BATCHED,
        DROPPED,
        GATE_FAILED,
        REFUSED,
        LANDED,
        DEPLOYED,
        HEALTHY,
    ];
}

pub(crate) mod batch_states {
    pub(crate) const OPEN: &str = "open";
    pub(crate) const CLOSED: &str = "closed";
    pub(crate) const GATING: &str = "gating";
    pub(crate) const REFUSED: &str = "refused";
    pub(crate) const LANDED: &str = "landed";
    pub(crate) const FAILED: &str = "failed";
    pub(crate) const ALL: [&str; 6] = [OPEN, CLOSED, GATING, REFUSED, LANDED, FAILED];
}

const TICK: Duration = Duration::from_secs(2 * 60);
const TOGGLE_POLL: Duration = Duration::from_secs(15);
const BACKOFF_FIRST: Duration = Duration::from_secs(5 * 60);
const BACKOFF_MAX: Duration = Duration::from_secs(4 * 60 * 60);

pub(crate) struct RepairContext<R, E, B> {
    pub(crate) persist: PersistHandle,
    pub(crate) runner: R,
    pub(crate) engineer: E,
    pub(crate) live: B,
    pub(crate) repo: RepoLayout,
    pub(crate) machine: Option<Uuid>,
    pub(crate) machine_name: Option<String>,
    pub(crate) shutdown: CancellationToken,
}

impl<R, E, B> RepairContext<R, E, B> {
    pub(crate) fn stopping(&self) -> bool {
        self.shutdown.is_cancelled()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TickReport {
    pub(crate) attempted: Option<(i64, AttemptEnd)>,
    pub(crate) batches: Vec<(i64, BatchEnd)>,
    pub(crate) deployed: Vec<i64>,
    pub(crate) healthy: Vec<i64>,
}

async fn is_fix_host<R, E, B>(ctx: &RepairContext<R, E, B>) -> Result<bool, PersistError> {
    let limits = ctx.persist.web().wright_followup_limits().await?;
    Ok(ctx
        .machine_name
        .as_deref()
        .is_some_and(|name| name.eq_ignore_ascii_case(limits.fix_host.trim())))
}

async fn host_work<R, E, B>(
    ctx: &RepairContext<R, E, B>,
    claims_open: bool,
    report: &mut TickReport,
) -> Result<(), PersistError>
where
    R: CommandRunner,
    E: RepairEngineer,
{
    let today = chrono::Utc::now().date_naive();
    batch::close_due(ctx, today).await?;
    report.batches = batch::land_due(ctx).await?;
    let Some(open) = batch::open_today(ctx, today).await? else {
        return Ok(());
    };
    let web = ctx.persist.web();
    if !claims_open || !web.live_wright_handoffs().await?.is_empty() {
        return Ok(());
    }
    let claimed = web.claim_wright_repairs(open.id, ctx.machine, 1).await?;
    if let Some(claim) = claimed.first() {
        report.attempted = Some((claim.id, attempt::attempt(ctx, claim).await?));
    }
    Ok(())
}

pub(crate) async fn tick<R, E, B>(
    ctx: &RepairContext<R, E, B>,
    claims_open: bool,
) -> Result<TickReport, PersistError>
where
    R: CommandRunner,
    E: RepairEngineer,
    B: LiveBuild,
{
    let mut report = TickReport::default();
    if is_fix_host(ctx).await? {
        host_work(ctx, claims_open, &mut report).await?;
    }
    let followed = follow::follow(ctx).await?;
    report.deployed = followed.deployed;
    report.healthy = followed.healthy;
    Ok(report)
}

struct Backoff {
    delay: Duration,
    until: Option<Instant>,
}

impl Backoff {
    fn new() -> Self {
        Self {
            delay: BACKOFF_FIRST,
            until: None,
        }
    }

    fn open(&self, now: Instant) -> bool {
        self.until.is_none_or(|until| now >= until)
    }

    fn note(&mut self, end: Option<AttemptEnd>, now: Instant) {
        match end {
            Some(AttemptEnd::NotStarted) => {
                self.until = Some(now + self.delay);
                self.delay = (self.delay * 2).min(BACKOFF_MAX);
            }
            Some(_) => *self = Self::new(),
            None => {}
        }
    }
}

type LiveContext = RepairContext<ProcessRunner, InvestigateEngineer, RunningBinary>;

static NO_REPOSITORY_TOLD: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

fn log_no_repository(error: &str, cwd: &std::path::Path) {
    if NO_REPOSITORY_TOLD.swap(true, std::sync::atomic::Ordering::AcqRel) {
        log_no_repository_again(error, cwd);
    } else {
        log_no_repository_first(error, cwd);
    }
}

fn log_no_repository_first(error: &str, cwd: &std::path::Path) {
    tracing::info!(%error, cwd = %cwd.display(), "wright repair: mike does not run from a checkout here, so this machine does not repair; said once per process");
}

fn log_no_repository_again(error: &str, cwd: &std::path::Path) {
    tracing::debug!(%error, cwd = %cwd.display(), "wright repair: no repository to repair from");
}

async fn live_context(
    persist: &PersistHandle,
    engineer: &mut Option<InvestigateEngineer>,
    shutdown: &CancellationToken,
) -> Option<LiveContext> {
    let cwd = std::env::current_dir().ok()?;
    let repo = match RepoLayout::discover(&ProcessRunner, &cwd).await {
        Ok(repo) => repo,
        Err(e) => {
            log_no_repository(&e.to_string(), &cwd);
            return None;
        }
    };
    let identity = crate::machine_identity::load_default().await.ok();
    let machine = identity.as_ref().map(|m| m.machine_uuid);
    let machine_name = identity.map(|m| m.machine_name);
    Some(RepairContext {
        persist: persist.clone(),
        runner: ProcessRunner,
        engineer: engineer.take()?,
        live: RunningBinary,
        repo,
        machine,
        machine_name,
        shutdown: shutdown.clone(),
    })
}

struct Worker {
    persist: PersistHandle,
    shutdown: CancellationToken,
    engineer: Option<InvestigateEngineer>,
    ctx: Option<LiveContext>,
    recovered: bool,
    backoff: Backoff,
}

impl Worker {
    async fn iteration(&mut self) {
        if self.ctx.is_none() {
            self.ctx = live_context(&self.persist, &mut self.engineer, &self.shutdown).await;
        }
        let Some(ctx) = self.ctx.as_ref() else {
            return;
        };
        if !self.recovered {
            self.recovered = recovered(ctx).await;
            if !self.recovered {
                return;
            }
        }
        run_tick(ctx, &mut self.backoff).await;
    }
}

async fn recovered(ctx: &LiveContext) -> bool {
    match follow::recover(ctx).await {
        Ok(interrupted) => {
            log_recovered(interrupted);
            true
        }
        Err(e) => {
            log_recover_failed(&e);
            false
        }
    }
}

async fn run_tick(ctx: &LiveContext, backoff: &mut Backoff) {
    match tick(ctx, backoff.open(Instant::now())).await {
        Ok(report) => {
            backoff.note(report.attempted.map(|(_, end)| end), Instant::now());
            log_tick(&report);
        }
        Err(e) => log_tick_failed(&e),
    }
}

fn log_recovered(interrupted: usize) {
    tracing::info!(
        interrupted,
        "wright repair: settled repairs a stop interrupted"
    );
}

fn log_recover_failed(error: &PersistError) {
    tracing::warn!(error = %error, "wright repair: recovering interrupted repairs failed");
}

fn log_tick_failed(error: &PersistError) {
    tracing::warn!(error = %error, "wright repair: the tick failed");
}

fn log_tick(report: &TickReport) {
    if report.attempted.is_some()
        || !report.batches.is_empty()
        || !report.deployed.is_empty()
        || !report.healthy.is_empty()
    {
        tracing::info!(
            attempted = ?report.attempted,
            batches = ?report.batches,
            deployed = ?report.deployed,
            healthy = ?report.healthy,
            "wright repair: tick"
        );
    }
}

async fn run(
    mut worker: Worker,
    toggles: Arc<ArcSwap<FeatureToggles>>,
    shutdown: CancellationToken,
) {
    loop {
        let pause = if toggles.load().wright {
            worker.iteration().await;
            TICK
        } else {
            TOGGLE_POLL
        };
        tokio::select! {
            biased;
            () = shutdown.cancelled() => return,
            () = tokio::time::sleep(pause) => {}
        }
    }
}

pub fn spawn_wright_repair_worker(state: &AppState) {
    let Some(persist) = state.persist().cloned() else {
        return tracing::debug!("wright repair: not spawning without persistence");
    };
    let worker = Worker {
        persist,
        shutdown: state.shutdown(),
        engineer: Some(InvestigateEngineer::for_state(state)),
        ctx: None,
        recovered: false,
        backoff: Backoff::new(),
    };
    let toggles = state.feature_toggle_overlay.clone();
    state.tracker.spawn(run(worker, toggles, state.shutdown()));
    tracing::info!(
        tick_secs = TICK.as_secs(),
        "wright repair: worker spawned; it idles while the wright toggle is off"
    );
}
