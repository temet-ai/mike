use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::circuit_breaker::CircuitBreaker;
use crate::state::AppState;
use crate::steerer::registry::SteererRegistry;
use crate::store::RequestStore;
use crate::store::persistence::PersistHandle;

use sweep::{SweepContext, run_sweep};

pub mod due_wakes;
pub mod label_tick;
pub mod sweep;
pub mod targets;
pub mod wake_recheck;

pub const SWEEP_INTERVAL: Duration = Duration::from_secs(60);

const SWEEP_BATCH: i64 = 50;

const CIRCUIT_BREAKER_THRESHOLD: u32 = 5;

const CIRCUIT_BREAKER_COOLDOWN_BASE_SECS: u64 = 60;

const CIRCUIT_BREAKER_COOLDOWN_MAX_SECS: u64 = 1800;

pub fn spawn_quiescence_worker(state: &AppState) {
    let Some(persist) = state.persist.clone() else {
        log_not_spawning_no_persistence();
        return;
    };

    let live_config = state.live_config.clone();
    let settings = state.settings_overlay.clone();
    let steerer_registry = state.steerer_registry.clone();
    let key_slot = state.steerer_openrouter_key.clone();
    let store = state.store.clone();
    let toggles = state.feature_toggle_overlay.clone();
    let shutdown = state.shutdown();
    let remote_control_streams = state.remote_control_streams.clone();
    let injection_gate = state.injection_gate.clone();
    let usage_sink = state.llm_usage_sink.clone();
    let embedding = state.embedding_service.clone();
    let lessons = state.lesson_hooks.clone();

    state.tracker.spawn(async move {
        run(RunArgs {
            persist,
            store,
            steerer_registry,
            key_slot,
            toggles,
            live_config,
            settings,
            shutdown,
            remote_control_streams,
            injection_gate,
            usage_sink,
            embedding,
            lessons,
        })
        .await;
    });

    tracing::info!("quiescence_worker: spawned");
}

fn log_not_spawning_no_persistence() {
    tracing::debug!("quiescence_worker: no persistence — not spawning");
}

struct RunArgs {
    persist: PersistHandle,
    store: Arc<RwLock<RequestStore>>,
    steerer_registry: Arc<ArcSwap<SteererRegistry>>,
    key_slot: crate::llm::key::OpenRouterKeySlot,
    toggles: Arc<ArcSwap<crate::feature_toggles::FeatureToggles>>,
    live_config: Arc<crate::config::live::LiveConfig>,
    settings: Arc<ArcSwap<crate::settings::SettingsOverlay>>,
    shutdown: CancellationToken,
    remote_control_streams: crate::proxy::remote_control::ReplyStreams,
    injection_gate: Arc<crate::injection::InjectionGate>,
    usage_sink: Option<Arc<dyn crate::llm::LlmUsageSink>>,
    embedding: crate::analysis::embeddings::EmbeddingService,
    lessons: crate::state::lesson_hooks::LessonHookCell,
}

async fn run(args: RunArgs) {
    let RunArgs {
        persist,
        store,
        steerer_registry,
        key_slot,
        toggles,
        live_config,
        settings,
        shutdown,
        remote_control_streams,
        injection_gate,
        usage_sink,
        embedding,
        lessons,
    } = args;

    log_starting(quiet_secs_of(&live_config.load().steerer));

    let mut interval = tokio::time::interval(SWEEP_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let mut breaker = CircuitBreaker::new(
        CIRCUIT_BREAKER_THRESHOLD,
        Duration::from_secs(CIRCUIT_BREAKER_COOLDOWN_BASE_SECS),
        Duration::from_secs(CIRCUIT_BREAKER_COOLDOWN_MAX_SECS),
    );

    let mut tick: u64 = 0;
    loop {
        tokio::select! {
            biased;

            _ = shutdown.cancelled() => {
                log_shutdown_signalled();
                return;
            }

            _ = interval.tick() => {
                tick += 1;
                let steerer_config = live_config.load().steerer.clone();
                let quiet_secs = quiet_secs_of(&steerer_config);
                let ctx = SweepContext {
                    persist: &persist,
                    store: &store,
                    steerer_registry: &steerer_registry,
                    key_slot: &key_slot,
                    config: &steerer_config,
                    settings: &settings,
                    remote_control_streams: &remote_control_streams,
                    injection_gate: &injection_gate,
                    usage_sink: usage_sink.as_ref(),
                    embedding: &embedding,
                    lessons: crate::state::lesson_hooks::hooks_in(&lessons),
                    toggles: &toggles,
                };
                let toggle_snapshot = toggles.load();
                run_guarded_tick(
                    &ctx,
                    tick,
                    toggle_snapshot.turn_guard,
                    toggle_snapshot.judge_lessons,
                    quiet_secs,
                    &mut breaker,
                )
                .await;
            }
        }
    }
}

fn quiet_secs_of(config: &crate::config::steerer::SteererConfig) -> i64 {
    i64::try_from(config.quiescence_minutes.saturating_mul(60)).unwrap_or(i64::MAX)
}

async fn run_guarded_tick(
    ctx: &SweepContext<'_>,
    tick: u64,
    guard_on: bool,
    proposer_on: bool,
    quiet_secs: i64,
    breaker: &mut CircuitBreaker,
) {
    if ctx.toggles.load().steerer {
        run_steerer_ticks(ctx, guard_on, quiet_secs, breaker).await;
    }
    if tick.is_multiple_of(due_wakes::DEMOTION_EVERY_TICKS) {
        crate::steerer::guard::promotion::demotion::run_demotion_check(
            ctx.persist,
            chrono::Utc::now(),
        )
        .await;
    }
    if label_tick::is_label_tick(tick) {
        label_tick::run_label_tick(ctx, guard_on).await;
    }
    ctx.lessons.proposer_tick(ctx, tick, proposer_on).await;
}

async fn run_steerer_ticks(
    ctx: &SweepContext<'_>,
    guard_on: bool,
    quiet_secs: i64,
    breaker: &mut CircuitBreaker,
) {
    if !ctx
        .persist
        .holds_llm_role(crate::llm::LlmRole::Steerer, SWEEP_INTERVAL)
        .await
    {
        return;
    }
    due_wakes::run_due_wakes(ctx, guard_on).await;
    run_one_tick(ctx, quiet_secs, breaker).await;
}

fn log_starting(quiet_secs: i64) {
    tracing::debug!(
        "quiescence_worker: starting periodic sweep (interval={}s, quiet_secs={quiet_secs}, batch={SWEEP_BATCH})",
        SWEEP_INTERVAL.as_secs(),
    );
}

fn log_shutdown_signalled() {
    tracing::debug!("quiescence_worker: shutdown signalled, exiting");
}

async fn run_one_tick(ctx: &SweepContext<'_>, quiet_secs: i64, breaker: &mut CircuitBreaker) {
    let now = Instant::now();
    if breaker.should_skip(now) {
        log_breaker_open();
        return;
    }
    let probing = breaker.is_probing(now);
    if probing {
        log_breaker_probing();
    }
    let batch = if probing { 1 } else { SWEEP_BATCH };
    run_sweep(ctx, quiet_secs, batch, breaker).await;
}

fn log_breaker_open() {
    tracing::debug!("quiescence_worker: circuit breaker open — skipping sweep");
}

fn log_breaker_probing() {
    tracing::info!("quiescence_worker: circuit breaker probing recovery");
}
