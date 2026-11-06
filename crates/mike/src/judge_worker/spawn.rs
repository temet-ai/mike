use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use tokio_util::sync::CancellationToken;

use crate::circuit_breaker::CircuitBreaker;
use crate::config::AppState;
use crate::feature_toggles::FeatureToggles;
use crate::llm::OpenRouterClient;
use crate::store::persistence::PersistHandle;

use crate::settings::resolve_judge_worker_llm_settings;

use super::immediate::ImmediateJudge;
use super::live_shared::LiveJudgeShared;
use super::live_tick::{LiveTick, run_live_tick};
use super::turn_sweep::{JudgeSweepParams, resolve_turn_judge_runtime, run_judge_sweep};

pub fn spawn_judge_worker(state: &AppState) {
    crate::llm::limiter::install_runtime(
        state.settings_overlay.clone(),
        state.llm_usage_sink.clone(),
    );

    let toggles = state.feature_toggle_overlay.load();
    if !toggles.judge_worker {
        log_not_spawning_toggle_off();
        return;
    }

    let Some(persist) = state.persist.clone() else {
        log_not_spawning_no_persistence();
        return;
    };

    let Some(llm) = state.judge_llm.clone() else {
        log_not_spawning_judge_disabled();
        return;
    };

    let judge_cfg = state.config().steerer.judge.clone();
    let interval_secs = judge_cfg.interval_secs;
    let backoff_base_secs = judge_cfg.backoff_base_secs;
    let backoff_max_secs = judge_cfg.backoff_max_secs;
    let circuit_breaker_threshold = judge_cfg.circuit_breaker_threshold;
    let circuit_breaker_cooldown_base_secs = judge_cfg.circuit_breaker_cooldown_base_secs;
    let circuit_breaker_cooldown_max_secs = judge_cfg.circuit_breaker_cooldown_max_secs;
    let fallback_model = state.config().steerer.model.clone();

    let key_slot = state.steerer_openrouter_key.clone();
    let toggles = state.feature_toggle_overlay.clone();
    let settings = state.settings_overlay.clone();
    let shutdown = state.shutdown();
    let shared = LiveJudgeShared::default();

    let immediate = ImmediateJudge {
        persist: persist.clone(),
        llm: llm.clone(),
        key_slot: key_slot.clone(),
        toggles: toggles.clone(),
        settings: settings.clone(),
        shared: shared.clone(),
        judge_cfg: judge_cfg.clone(),
        shutdown: shutdown.clone(),
    };
    state
        .tracker
        .spawn(immediate.run(state.event_tx.subscribe()));

    state.tracker.spawn(async move {
        run(RunArgs {
            persist,
            llm,
            key_slot,
            toggles,
            settings,
            judge_cfg,
            fallback_model,
            backoff_base_secs,
            backoff_max_secs,
            circuit_breaker_threshold,
            circuit_breaker_cooldown_base_secs,
            circuit_breaker_cooldown_max_secs,
            interval_secs,
            shutdown,
            shared,
        })
        .await;
    });

    log_spawned(interval_secs);
}

fn log_not_spawning_toggle_off() {
    tracing::info!(
        "judge_worker: not spawning — the judge_worker feature toggle is off (enable it on /settings)"
    );
}

fn log_not_spawning_no_persistence() {
    tracing::info!("judge_worker: not spawning — no database persistence is wired");
}

fn log_not_spawning_judge_disabled() {
    tracing::info!(
        "judge_worker: not spawning — the judge LLM client failed to build at startup (see the earlier warning)"
    );
}

fn log_spawned(interval_secs: u64) {
    tracing::info!(interval_secs, "judge_worker: spawned");
}

struct RunArgs {
    persist: PersistHandle,
    llm: OpenRouterClient,
    key_slot: crate::llm::key::OpenRouterKeySlot,
    toggles: Arc<ArcSwap<FeatureToggles>>,
    settings: Arc<ArcSwap<crate::settings::SettingsOverlay>>,
    judge_cfg: crate::steerer::config::JudgeConfig,
    fallback_model: String,
    backoff_base_secs: i64,
    backoff_max_secs: i64,
    circuit_breaker_threshold: u32,
    circuit_breaker_cooldown_base_secs: u64,
    circuit_breaker_cooldown_max_secs: u64,
    interval_secs: u64,
    shutdown: CancellationToken,
    shared: LiveJudgeShared,
}

struct JudgeTickContext<'a> {
    persist: &'a PersistHandle,
    llm: &'a OpenRouterClient,
    key_slot: &'a crate::llm::key::OpenRouterKeySlot,
    settings: &'a Arc<ArcSwap<crate::settings::SettingsOverlay>>,
    judge_cfg: &'a crate::steerer::config::JudgeConfig,
    fallback_model: &'a str,
    backoff_base_secs: i64,
    backoff_max_secs: i64,
    shared: &'a LiveJudgeShared,
    interval: Duration,
}

async fn run(args: RunArgs) {
    let RunArgs {
        persist,
        llm,
        key_slot,
        toggles,
        settings,
        judge_cfg,
        fallback_model,
        backoff_base_secs,
        backoff_max_secs,
        circuit_breaker_threshold,
        circuit_breaker_cooldown_base_secs,
        circuit_breaker_cooldown_max_secs,
        interval_secs,
        shutdown,
        shared,
    } = args;

    log_starting(interval_secs);

    let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let mut breaker = CircuitBreaker::new(
        circuit_breaker_threshold,
        Duration::from_secs(circuit_breaker_cooldown_base_secs),
        Duration::from_secs(circuit_breaker_cooldown_max_secs),
    );

    let ctx = JudgeTickContext {
        persist: &persist,
        llm: &llm,
        key_slot: &key_slot,
        settings: &settings,
        judge_cfg: &judge_cfg,
        fallback_model: &fallback_model,
        backoff_base_secs,
        backoff_max_secs,
        shared: &shared,
        interval: Duration::from_secs(interval_secs),
    };

    loop {
        tokio::select! {
            biased;

            _ = shutdown.cancelled() => {
                log_shutdown_signalled();
                return;
            }

            _ = interval.tick() => {
                if tick_then_wait_out_defer(&ctx, &toggles, &shutdown, &mut breaker).await.is_break() {
                    return;
                }
            }
        }
    }
}

fn log_starting(interval_secs: u64) {
    tracing::debug!(
        "judge_worker: starting periodic sweep (interval={}s)",
        interval_secs
    );
}

fn log_shutdown_signalled() {
    tracing::debug!("judge_worker: shutdown signalled, exiting");
}

fn log_disabled_via_toggle() {
    tracing::info!(
        "judge_worker: disabled via feature toggle — exiting (a UI flip back on needs a restart)"
    );
}

async fn tick_then_wait_out_defer(
    ctx: &JudgeTickContext<'_>,
    toggles: &Arc<ArcSwap<FeatureToggles>>,
    shutdown: &CancellationToken,
    breaker: &mut CircuitBreaker,
) -> ControlFlow<()> {
    if !toggles.load().judge_worker {
        log_disabled_via_toggle();
        return ControlFlow::Break(());
    }
    if !ctx
        .persist
        .holds_llm_role(crate::llm::LlmRole::JudgeWorker, ctx.interval)
        .await
    {
        return ControlFlow::Continue(());
    }
    let Some(defer) = run_one_tick(ctx, breaker).await else {
        return ControlFlow::Continue(());
    };
    if wait_out_defer(shutdown, defer).await {
        log_shutdown_signalled();
        return ControlFlow::Break(());
    }
    ControlFlow::Continue(())
}

async fn wait_out_defer(shutdown: &CancellationToken, defer: Duration) -> bool {
    tokio::select! {
        biased;
        _ = shutdown.cancelled() => true,
        _ = tokio::time::sleep(defer) => false,
    }
}

async fn run_one_tick(
    ctx: &JudgeTickContext<'_>,
    breaker: &mut CircuitBreaker,
) -> Option<Duration> {
    let live_settings = ctx.settings.load();
    if live_settings.turn_judge_live_bundle_id > 0 {
        let Some(live_llm) = crate::llm::key::keyed_client(ctx.llm, ctx.key_slot) else {
            log_client_not_built();
            return None;
        };
        let tick = LiveTick {
            persist: ctx.persist,
            llm: &live_llm,
            shared: ctx.shared,
            overlay: &live_settings,
            judge_cfg: ctx.judge_cfg,
        };
        return run_live_tick(tick, breaker).await.defer;
    }
    let runtime = resolve_turn_judge_runtime(&live_settings, ctx.judge_cfg, ctx.fallback_model);
    let Some(effective_llm) =
        crate::llm::key::effective_client_for(ctx.llm, ctx.key_slot, &runtime.model)
    else {
        log_no_api_key(&runtime.model);
        return None;
    };
    let effective_llm =
        effective_llm.with_runtime_settings(resolve_judge_worker_llm_settings(&live_settings));

    let params = JudgeSweepParams {
        model_label: &runtime.model,
        system_prompt: &runtime.system_prompt,
        rubric_version: &runtime.rubric_version,
        batch: runtime.batch_size,
        threshold: runtime.confidence_threshold,
        backoff_base_secs: ctx.backoff_base_secs,
        backoff_max_secs: ctx.backoff_max_secs,
        bundle_id: live_settings.bundles.turn_judge,
    };
    run_sweep_and_log(ctx.persist, &effective_llm, breaker, &params).await
}

fn log_no_api_key(model: &str) {
    tracing::warn!(
        model,
        "judge_worker: the model runs on OpenRouter and no OpenRouter API key is configured — skipping this sweep"
    );
}

fn log_client_not_built() {
    tracing::warn!("judge_worker: the judge LLM client could not be built — skipping this sweep");
}

async fn run_sweep_and_log(
    persist: &PersistHandle,
    llm: &OpenRouterClient,
    breaker: &mut CircuitBreaker,
    params: &JudgeSweepParams<'_>,
) -> Option<Duration> {
    match run_judge_sweep(persist, llm, breaker, params).await {
        Ok(report) => {
            if report.labeled > 0 {
                log_labeled(report.labeled);
            }
            report.defer
        }
        Err(e) => {
            log_sweep_error(&e);
            None
        }
    }
}

fn log_labeled(n: usize) {
    tracing::info!(labeled = n, "judge_worker: labeled {} turn(s)", n);
}

fn log_sweep_error(e: &anyhow::Error) {
    tracing::warn!(error = %e, "judge_worker: sweep error — will retry next interval");
}
