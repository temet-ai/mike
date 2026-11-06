use std::collections::HashSet;
use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use chrono::Utc;
use tokio::sync::broadcast;
use tokio::time::{Instant, sleep_until};
use tokio_util::sync::CancellationToken;

use super::build::{BUILDER_VERSION, BuildConfig, BuildReport, build_sessions};
use crate::circuit_breaker::CircuitBreaker;
use crate::config::AppState;
use crate::feature_toggles::FeatureToggles;
use crate::settings::meta_dossier::{
    BACKSTOP_INTERVAL_SECS_KEY, BACKSTOP_LOOKBACK_HOURS_KEY, BATCH_SESSIONS_KEY,
    DEFAULT_BACKSTOP_INTERVAL_SECS, DEFAULT_BACKSTOP_LOOKBACK_HOURS, DEFAULT_BATCH_SESSIONS,
    DEFAULT_FLUSH_INTERVAL_MS, DEFAULT_SESSION_SETTLE_MINUTES, FLUSH_INTERVAL_MS_KEY,
    SESSION_SETTLE_MINUTES_KEY,
};
use crate::store::models::ProxyEvent;
use crate::store::persistence::turn_facts::CandidateWindow;
use crate::store::persistence::{PersistError, PersistHandle};

const BREAKER_THRESHOLD: u32 = 5;
const BREAKER_COOLDOWN_BASE: Duration = Duration::from_secs(30);
const BREAKER_COOLDOWN_MAX: Duration = Duration::from_secs(600);

pub struct DossierWorkerDeps {
    pub persist: PersistHandle,
    pub toggles: Arc<ArcSwap<FeatureToggles>>,
    pub shutdown: CancellationToken,
}

pub fn spawn_dossier_builder(state: &AppState) {
    let Some(persist) = state.persist().cloned() else {
        tracing::debug!("dossier builder: no persistence, not spawning");
        return;
    };
    let rx = state.event_tx.subscribe();
    let deps = DossierWorkerDeps {
        persist,
        toggles: state.feature_toggle_overlay.clone(),
        shutdown: state.shutdown(),
    };
    state.tracker.spawn(async move {
        run_dossier_builder(rx, deps).await;
    });
    tracing::info!("dossier builder: spawned");
}

struct BuilderLoop {
    deps: DossierWorkerDeps,
    runtime: DossierRuntime,
    breaker: CircuitBreaker,
    pending_turns: HashSet<i64>,
    flush_at: Option<Instant>,
    backstop_at: Instant,
}

impl BuilderLoop {
    async fn start(deps: DossierWorkerDeps) -> Self {
        let runtime = DossierRuntime::read(&deps.persist, DossierRuntime::default()).await;
        let backstop_at = Instant::now() + runtime.backstop_interval;
        Self {
            deps,
            runtime,
            breaker: CircuitBreaker::new(
                BREAKER_THRESHOLD,
                BREAKER_COOLDOWN_BASE,
                BREAKER_COOLDOWN_MAX,
            ),
            pending_turns: HashSet::new(),
            flush_at: None,
            backstop_at,
        }
    }

    fn enabled(&self) -> bool {
        self.deps.toggles.load().dossier_builder
    }

    fn on_event(&mut self, event: ProxyEvent) {
        if let ProxyEvent::OperatorTurnClosed { closed_turn_id, .. } = event {
            self.pending_turns.insert(closed_turn_id);
            self.flush_at
                .get_or_insert_with(|| Instant::now() + self.runtime.flush_interval);
        }
    }

    fn next_deadline(&self) -> Instant {
        self.flush_at
            .map_or(self.backstop_at, |flush| flush.min(self.backstop_at))
    }

    async fn run_due(&mut self) {
        let now = Instant::now();
        if self.flush_at.is_some_and(|flush| flush <= now) {
            self.flush_at = None;
            self.flush().await;
        }
        if self.backstop_at <= now {
            self.backstop().await;
            self.backstop_at = Instant::now() + self.runtime.backstop_interval;
        }
    }

    async fn flush(&mut self) {
        let turns: Vec<i64> = self.pending_turns.drain().collect();
        if turns.is_empty() || !self.enabled() {
            return;
        }
        match self.deps.persist.sessions_for_turns(&turns).await {
            Ok(sessions) => self.build_in_batches(&sessions).await,
            Err(e) => tracing::warn!(error = %e, "dossier builder: session lookup failed"),
        }
    }

    async fn backstop(&mut self) {
        self.runtime = DossierRuntime::read(&self.deps.persist, self.runtime).await;
        if !self.enabled() {
            return;
        }
        let now = Utc::now();
        let window = CandidateWindow {
            lookback_from: now - self.runtime.lookback,
            settled_before: now - self.runtime.settle,
            limit: self.runtime.batch_sessions,
        };
        match self
            .deps
            .persist
            .find_turn_facts_candidates(BUILDER_VERSION, window)
            .await
        {
            Ok(sessions) => self.build_in_batches(&sessions).await,
            Err(e) => tracing::warn!(error = %e, "dossier builder: backstop query failed"),
        }
    }

    async fn build_in_batches(&mut self, sessions: &[i64]) {
        let size = usize::try_from(self.runtime.batch_sessions.max(1)).unwrap_or(1);
        for chunk in sessions.chunks(size) {
            self.build(chunk).await;
        }
    }

    async fn build(&mut self, sessions: &[i64]) {
        if self.breaker.should_skip(Instant::now().into_std()) {
            tracing::debug!("dossier builder: circuit open, leaving sessions to the backstop");
            return;
        }
        let cfg = BuildConfig {
            settle: self.runtime.settle,
        };
        let outcome = build_sessions(&self.deps.persist, sessions, Utc::now(), &cfg).await;
        self.breaker
            .record(outcome.is_ok(), Instant::now().into_std());
        log_outcome(sessions.len(), &outcome);
    }
}

fn log_outcome(sessions: usize, outcome: &Result<BuildReport, PersistError>) {
    match outcome {
        Ok(report) => log_built(sessions, report),
        Err(e) => tracing::warn!(error = %e, "dossier builder: batch failed"),
    }
}

fn log_built(sessions: usize, report: &BuildReport) {
    tracing::debug!(
        sessions,
        inserted = report.inserted,
        updated = report.updated,
        reclassified = report.reclassified,
        skipped_stale = report.skipped_stale,
        "dossier builder: batch built"
    );
}

pub async fn build_for_claimed_turns(
    persist: &PersistHandle,
    turn_ids: &[i64],
) -> Result<BuildReport, PersistError> {
    if turn_ids.is_empty() || !persist.dossier_builder_enabled().await? {
        return Ok(BuildReport::default());
    }
    let sessions = persist.sessions_for_turns(turn_ids).await?;
    build_sessions_now(persist, &sessions).await
}

pub async fn build_for_sessions(
    persist: &PersistHandle,
    session_ids: &[i64],
) -> Result<BuildReport, PersistError> {
    if session_ids.is_empty() || !persist.dossier_builder_enabled().await? {
        return Ok(BuildReport::default());
    }
    build_sessions_now(persist, session_ids).await
}

async fn build_sessions_now(
    persist: &PersistHandle,
    session_ids: &[i64],
) -> Result<BuildReport, PersistError> {
    let runtime = DossierRuntime::read(persist, DossierRuntime::default()).await;
    let cfg = BuildConfig {
        settle: runtime.settle,
    };
    build_sessions(persist, session_ids, Utc::now(), &cfg).await
}

fn handle_received(
    worker: &mut BuilderLoop,
    received: Result<ProxyEvent, broadcast::error::RecvError>,
) -> ControlFlow<()> {
    match received {
        Ok(event) => {
            worker.on_event(event);
            ControlFlow::Continue(())
        }
        Err(broadcast::error::RecvError::Lagged(skipped)) => {
            tracing::warn!(skipped, "dossier builder: broadcast lagged");
            ControlFlow::Continue(())
        }
        Err(broadcast::error::RecvError::Closed) => ControlFlow::Break(()),
    }
}

async fn worker_tick(
    worker: &mut BuilderLoop,
    rx: &mut broadcast::Receiver<ProxyEvent>,
) -> ControlFlow<()> {
    let deadline = worker.next_deadline();
    tokio::select! {
        biased;
        () = worker.deps.shutdown.cancelled() => ControlFlow::Break(()),
        received = rx.recv() => handle_received(worker, received),
        () = sleep_until(deadline) => {
            worker.run_due().await;
            ControlFlow::Continue(())
        }
    }
}

pub async fn run_dossier_builder(mut rx: broadcast::Receiver<ProxyEvent>, deps: DossierWorkerDeps) {
    let mut worker = BuilderLoop::start(deps).await;
    while worker_tick(&mut worker, &mut rx).await.is_continue() {}
    tracing::info!("dossier builder: stopped");
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DossierRuntime {
    pub flush_interval: Duration,
    pub batch_sessions: i64,
    pub backstop_interval: Duration,
    pub lookback: chrono::Duration,
    pub settle: chrono::Duration,
}

fn positive(value: Option<serde_json::Value>, fallback: i64) -> i64 {
    value
        .and_then(|value| value.as_i64())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

async fn read_int(persist: &PersistHandle, key: &str, fallback: i64) -> i64 {
    match persist.setting_value(key).await {
        Ok(value) => positive(value, fallback),
        Err(e) => {
            tracing::warn!(key, error = %e, "dossier builder: setting read failed, using the previous value");
            fallback
        }
    }
}

fn seconds(value: i64) -> Duration {
    Duration::from_secs(u64::try_from(value).unwrap_or(1))
}

fn millis(value: i64) -> Duration {
    Duration::from_millis(u64::try_from(value).unwrap_or(1))
}

impl Default for DossierRuntime {
    fn default() -> Self {
        Self {
            flush_interval: millis(DEFAULT_FLUSH_INTERVAL_MS),
            batch_sessions: DEFAULT_BATCH_SESSIONS,
            backstop_interval: seconds(DEFAULT_BACKSTOP_INTERVAL_SECS),
            lookback: chrono::Duration::hours(DEFAULT_BACKSTOP_LOOKBACK_HOURS),
            settle: chrono::Duration::minutes(DEFAULT_SESSION_SETTLE_MINUTES),
        }
    }
}

impl DossierRuntime {
    pub async fn read(persist: &PersistHandle, previous: DossierRuntime) -> DossierRuntime {
        let flush_ms = read_int(
            persist,
            FLUSH_INTERVAL_MS_KEY,
            i64::try_from(previous.flush_interval.as_millis()).unwrap_or(DEFAULT_FLUSH_INTERVAL_MS),
        )
        .await;
        let batch = read_int(persist, BATCH_SESSIONS_KEY, previous.batch_sessions).await;
        let backstop_secs = read_int(
            persist,
            BACKSTOP_INTERVAL_SECS_KEY,
            i64::try_from(previous.backstop_interval.as_secs())
                .unwrap_or(DEFAULT_BACKSTOP_INTERVAL_SECS),
        )
        .await;
        let lookback_hours = read_int(
            persist,
            BACKSTOP_LOOKBACK_HOURS_KEY,
            previous.lookback.num_hours(),
        )
        .await;
        let settle_minutes = read_int(
            persist,
            SESSION_SETTLE_MINUTES_KEY,
            previous.settle.num_minutes(),
        )
        .await;
        DossierRuntime {
            flush_interval: millis(flush_ms),
            batch_sessions: batch,
            backstop_interval: seconds(backstop_secs),
            lookback: chrono::Duration::hours(lookback_hours),
            settle: chrono::Duration::minutes(settle_minutes),
        }
    }
}
