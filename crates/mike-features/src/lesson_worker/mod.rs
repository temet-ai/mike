use crate::store::persistence::lessons_repo::LessonsPersist as _;
pub mod action_record;
pub mod gc;
pub mod nightly;
pub mod refresh;
pub mod relevance;
pub mod session_consolidation;
pub mod snapshot;
pub mod vector_repair;
#[cfg(test)]
pub mod vector_repair_tests;

#[cfg(test)]
pub mod session_consolidation_tests;
#[cfg(test)]
pub mod snapshot_tests;

use std::sync::Arc;
use std::time::Instant;

use arc_swap::ArcSwap;
use chrono::{DateTime, Local, TimeZone, Timelike, Utc};
use tokio_util::sync::CancellationToken;

use crate::analysis::EmbeddingService;
use crate::circuit_breaker::CircuitBreaker;
use crate::config::LessonWorkerConfig;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;

pub use snapshot::{InjectableLesson, LessonSnapshot};

/// Consecutive failed session-consolidation ticks before the breaker
/// trips and skips embed_documents calls entirely for a cooldown window.
const CIRCUIT_BREAKER_THRESHOLD: u32 = 3;

/// Initial breaker cooldown once tripped; doubles on repeated probe
/// failure up to [`CIRCUIT_BREAKER_COOLDOWN_MAX_SECS`].
const CIRCUIT_BREAKER_COOLDOWN_BASE_SECS: u64 = 300;

/// Ceiling on the breaker cooldown.
const CIRCUIT_BREAKER_COOLDOWN_MAX_SECS: u64 = 3600;

/// Session consolidation poll interval — check for idle raw lessons
/// every 30 seconds regardless of `session_consolidation_idle_secs`.
/// The idle threshold itself is applied in the discovery SQL (`created_at < $1`).
const SESSION_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

/// Shared handle type stored on [`AppState`]. Hot-swappable via
/// `ArcSwap::store`; cloned cheaply onto the proxy hot path.
pub type LessonCache = Arc<ArcSwap<LessonSnapshot>>;

/// Spawn the lesson worker onto the shared `TaskTracker`. Returns
/// immediately — the worker runs in the background.
///
/// No-op when the `lesson_worker` feature toggle is off (default ON) or when
/// persistence is not wired (no Postgres connection to query).
///
/// [`run`] re-reads the `lesson_worker` toggle itself on every tick and
/// exits cleanly the first time it reads `false` — a UI flip to off stops
/// this worker within one [`SESSION_POLL_INTERVAL`], no restart required.
/// Flipping it back on afterward does need a restart: the loop has already
/// exited and nothing re-spawns it while the process stays up.
pub fn spawn_lesson_worker(state: &AppState) {
    // Spawn-time gate: skip spawning entirely when off at boot. The live
    // toggle is re-read every tick inside run(), so a later flip to off is
    // caught without needing this initial check to repeat.
    let Some(persist) = state.persist.clone() else {
        tracing::debug!("lesson_worker: persistence not wired — skipping spawn");
        return;
    };

    let embedding = state.embedding_service();
    let shutdown = state.shutdown();
    let config = state.config.lesson_worker.clone();
    let cache = state.lesson_cache.clone();
    let toggles = state.feature_toggle_overlay.clone();

    state.tracker.spawn(async move {
        run(persist, embedding, shutdown, config, cache, toggles).await;
    });
}

/// UTC instant for local midnight on `now`'s calendar date. The DB-backed
/// "did the nightly already run today" check needs a fixed instant, not a
/// local calendar day, so the app's timezone and the DB server's can never
/// silently disagree. `.earliest()` picks the first occurrence on a
/// DST-ambiguous local midnight; only a nonexistent local time (a
/// spring-forward gap exactly at midnight) yields `None`, and callers treat
/// that as "skip this check, try again next tick".
fn local_midnight_utc(now: DateTime<Local>) -> Option<DateTime<Utc>> {
    let naive_midnight = now.date_naive().and_hms_opt(0, 0, 0)?;
    Local
        .from_local_datetime(&naive_midnight)
        .earliest()
        .map(|dt| dt.with_timezone(&Utc))
}

/// Dependencies for one scheduled tick, stable across every tick of
/// [`run`]'s loop. Bundled to keep [`run_scheduled_tick`]'s signature
/// small — mirrors `judge_worker::session_poll::SessionPollDeps`'s role
/// for the sibling session-judge worker.
struct TickCtx<'a> {
    persist: &'a PersistHandle,
    embedding: &'a EmbeddingService,
    cache: &'a LessonCache,
    config: &'a LessonWorkerConfig,
}

async fn run(
    persist: PersistHandle,
    embedding: EmbeddingService,
    shutdown: CancellationToken,
    config: LessonWorkerConfig,
    cache: LessonCache,
    toggles: Arc<ArcSwap<crate::feature_toggles::FeatureToggles>>,
) {
    tracing::info!("lesson_worker: starting");

    refresh::refresh_lesson_cache_inner(&persist, &cache).await;
    log_embedding_state_at_startup(&embedding);
    run_startup_if_enabled(&persist, &config, &toggles).await;

    let mut session_interval = tokio::time::interval(SESSION_POLL_INTERVAL);
    // Avoid thundering-herd at startup — miss the first tick rather than
    // immediately running the consolidation when the process boots.
    session_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let gc_interval_duration = std::time::Duration::from_secs(config.gc_interval_secs.max(1));
    // Force GC on first tick by pretending the last run was gc_interval ago.
    // The subtraction may underflow on very old Instants, so default to now
    // (GC waits one full interval before running). Either is correct.
    let mut last_gc = Instant::now()
        .checked_sub(gc_interval_duration)
        .unwrap_or_else(Instant::now);

    // Circuit breaker over session consolidation's embed_documents calls
    // only — GC and nightly consolidation are DB-only and unaffected by
    // the embedding dependency being down. Lives for the worker's whole
    // lifetime, mirroring judge_worker's loop-local breaker.
    let mut embed_breaker = vector_repair::EmbeddingWork::new();

    let ctx = TickCtx {
        persist: &persist,
        embedding: &embedding,
        cache: &cache,
        config: &config,
    };

    loop {
        tokio::select! {
            biased;

            _ = shutdown.cancelled() => {
                log_shutdown_signalled();
                return;
            }

            _ = session_interval.tick() => {
                run_enabled_tick(&ctx, &toggles, gc_interval_duration, &mut last_gc, &mut embed_breaker).await;
            }
        }
    }
}

async fn run_startup_if_enabled(
    persist: &PersistHandle,
    config: &LessonWorkerConfig,
    toggles: &ArcSwap<crate::feature_toggles::FeatureToggles>,
) {
    if toggles.load().lesson_worker {
        run_startup_nightly_catchup(persist, config).await;
    }
}

async fn run_enabled_tick(
    ctx: &TickCtx<'_>,
    toggles: &ArcSwap<crate::feature_toggles::FeatureToggles>,
    gc_interval_duration: std::time::Duration,
    last_gc: &mut Instant,
    embed_breaker: &mut vector_repair::EmbeddingWork,
) {
    if !toggles.load().lesson_worker {
        log_disabled_via_toggle();
        return;
    }
    run_scheduled_tick(ctx, gc_interval_duration, last_gc, embed_breaker).await;
}

/// Logged when shutdown fires between ticks — see [`run`].
fn log_shutdown_signalled() {
    tracing::debug!("lesson_worker: shutdown signalled, exiting");
}

/// Logged when a tick observes the `lesson_worker` feature toggle flipped
/// off and exits the loop — see [`run`].
fn log_disabled_via_toggle() {
    tracing::info!("lesson_worker: disabled via feature toggle — paused until enabled");
}

/// Logged once at startup — see [`run`].
fn log_embedding_state_at_startup(embedding: &EmbeddingService) {
    if !embedding.is_available() {
        log_embedding_disabled_at_startup();
    }
}

/// Logged when the embedding service starts disabled — see
/// [`log_embedding_state_at_startup`].
fn log_embedding_disabled_at_startup() {
    tracing::info!(
        "lesson_worker: embedding service disabled at startup — raw-lesson consolidation paused; GC and lesson-cache refresh still run"
    );
}

/// True when `now`'s local hour is at/after `nightly_hour` — the startup
/// catch-up test: has today's nightly window already passed.
fn is_at_or_after_nightly_hour(now: DateTime<Local>, nightly_hour: u8) -> bool {
    now.hour() >= u32::from(nightly_hour)
}

/// True when `now`'s local hour exactly matches `nightly_hour` — the
/// per-tick scheduled-window test (only touches the DB once per hour).
fn is_nightly_hour(now: DateTime<Local>, nightly_hour: u8) -> bool {
    now.hour() == u32::from(nightly_hour)
}

/// Run nightly consolidation now if it has not already run since local
/// midnight on `now`'s date. Shared by the startup catch-up and the
/// per-tick scheduled check — only `trigger` (recorded on the
/// consolidation run) differs between them.
async fn run_nightly_if_due(
    persist: &PersistHandle,
    now: DateTime<Local>,
    max_lessons_per_project: i64,
    trigger: &str,
) {
    let Some(since) = local_midnight_utc(now) else {
        log_local_midnight_unresolved(trigger);
        return;
    };
    match persist.lessons().nightly_ran_since(since).await {
        Ok(false) => {
            nightly::run_nightly_consolidation(persist, max_lessons_per_project, trigger).await;
        }
        Ok(true) => {}
        Err(e) => log_nightly_ran_check_failed(trigger, &e),
    }
}

/// Logged when today's local midnight cannot be resolved — see
/// [`run_nightly_if_due`].
fn log_local_midnight_unresolved(trigger: &str) {
    tracing::warn!(trigger, "lesson_worker: could not resolve local midnight");
}

/// Logged when checking whether the nightly pass already ran fails — see
/// [`run_nightly_if_due`].
fn log_nightly_ran_check_failed(trigger: &str, e: &crate::store::persistence::PersistError) {
    tracing::warn!(
        error = %e,
        trigger,
        "lesson_worker: failed to check nightly-ran state"
    );
}

/// Startup catch-up: if today's nightly hour has already passed with no
/// successful run recorded, run one now instead of silently waiting for
/// tomorrow's window — see [`run`].
async fn run_startup_nightly_catchup(persist: &PersistHandle, config: &LessonWorkerConfig) {
    let now = Local::now();
    if is_at_or_after_nightly_hour(now, config.nightly_consolidation_hour) {
        run_nightly_if_due(
            persist,
            now,
            config.max_lessons_per_project,
            "startup_catchup",
        )
        .await;
    }
}

/// Per-tick nightly check: only touches the DB when the hour actually
/// matches — the ran-today gate is one EXISTS lookup, not a per-tick cost.
async fn maybe_run_nightly(persist: &PersistHandle, config: &LessonWorkerConfig) {
    let now = Local::now();
    if is_nightly_hour(now, config.nightly_consolidation_hour) {
        run_nightly_if_due(persist, now, config.max_lessons_per_project, "schedule").await;
    }
}

/// Per-tick GC check.
async fn maybe_run_gc(
    persist: &PersistHandle,
    gc_interval_duration: std::time::Duration,
    last_gc: &mut Instant,
) {
    if last_gc.elapsed() >= gc_interval_duration {
        gc::run_gc(persist).await;
        *last_gc = Instant::now();
    }
}

async fn maybe_run_session_consolidation(ctx: &TickCtx<'_>, embed_breaker: &mut CircuitBreaker) {
    if !ctx.embedding.is_available() {
        return;
    }
    let breaker_now = Instant::now();
    if embed_breaker.should_skip(breaker_now) {
        log_breaker_open();
        return;
    }
    if embed_breaker.is_probing(breaker_now) {
        log_breaker_probing();
    }
    let embed_call_failed = session_consolidation::run_session_consolidation(
        ctx.persist,
        ctx.embedding,
        ctx.config.session_consolidation_idle_secs,
        ctx.config.dedup_threshold,
    )
    .await;
    embed_breaker.record(!embed_call_failed, Instant::now());
}

fn log_breaker_open() {
    tracing::debug!("lesson_worker: circuit breaker open — skipping session consolidation");
}

fn log_breaker_probing() {
    tracing::info!("lesson_worker: circuit breaker probing recovery");
}

async fn run_scheduled_tick(
    ctx: &TickCtx<'_>,
    gc_interval_duration: std::time::Duration,
    last_gc: &mut Instant,
    embed_breaker: &mut vector_repair::EmbeddingWork,
) {
    maybe_run_nightly(ctx.persist, ctx.config).await;
    maybe_run_gc(ctx.persist, gc_interval_duration, last_gc).await;
    embed_breaker.repair(ctx.persist, ctx.embedding).await;
    maybe_run_session_consolidation(ctx, &mut embed_breaker.consolidation).await;

    refresh::refresh_lesson_cache_inner(ctx.persist, ctx.cache).await;
    let snapshot = ctx.cache.load_full();
    snapshot
        .relevance
        .refresh(ctx.embedding, ctx.persist, &snapshot)
        .await;
}

pub async fn refresh_lesson_cache(state: &AppState) {
    let Some(persist) = state.persist.as_ref() else {
        return;
    };
    refresh::refresh_lesson_cache_inner(persist, &state.lesson_cache).await;
}

pub async fn consolidate_once(state: &AppState) {
    let Some(persist) = state.persist.as_ref() else {
        return;
    };
    session_consolidation::run_session_consolidation(
        persist,
        &state.embedding_service(),
        state.config.lesson_worker.session_consolidation_idle_secs,
        state.config.lesson_worker.dedup_threshold,
    )
    .await;
}

pub async fn nightly_once(state: &AppState) {
    let Some(persist) = state.persist.as_ref() else {
        return;
    };
    nightly::run_nightly_consolidation(
        persist,
        state.config.lesson_worker.max_lessons_per_project,
        "manual",
    )
    .await;
}

pub async fn gc_once(state: &AppState) {
    let Some(persist) = state.persist.as_ref() else {
        return;
    };
    gc::run_gc(persist).await;
}
