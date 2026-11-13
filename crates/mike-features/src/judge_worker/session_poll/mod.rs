pub mod cycle;
pub mod digest;
pub mod digest_repeat;
pub mod related;
pub mod verdict_write;

#[cfg(test)]
pub mod digest_tests;
#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use tokio_util::sync::CancellationToken;

use crate::analysis::EmbeddingService;
use crate::circuit_breaker::CircuitBreaker;
use crate::config::session_judge::SessionJudgeConfig;
use crate::feature_toggles::FeatureToggles;
use crate::llm::{LlmRole, OpenRouterClient};
use crate::state::AppState;
use crate::store::persistence::PersistHandle;

use cycle::{SessionPollDeps, run_poll};
use digest::SESSION_JUDGE_SYSTEM_PROMPT;

pub fn spawn_session_judge_worker(state: &AppState) {
    crate::llm::limiter::install_runtime(
        state.settings_overlay.clone(),
        state.llm_usage_sink.clone(),
    );

    if !state.feature_toggle_overlay.load().session_judge {
        return;
    }

    let Some(persist) = state.persist.clone() else {
        log_not_spawning_no_persistence();
        return;
    };

    let config = state.config.session_judge.clone();

    let llm = match crate::llm::OpenRouterClient::new(
        String::new(),
        config.model.clone(),
        None,
        Some(Duration::from_secs(config.request_timeout_secs)),
    ) {
        Ok(client) => client,
        Err(e) => {
            log_client_build_failed(&e);
            return;
        }
    };
    let llm = llm
        .with_attribution(LlmRole::SessionJudge, state.llm_usage_sink.as_ref())
        .with_live_settings(crate::settings::session_judge_live_source(
            &state.settings_overlay,
        ));

    let key_slot = state.steerer_openrouter_key.clone();
    let embedding = state.embedding_service();
    let toggles = state.feature_toggle_overlay.clone();
    let settings = state.settings_overlay.clone();
    let shutdown = state.shutdown();

    state.tracker.spawn(async move {
        run_session_loop(SessionJudgeWorkerDeps {
            persist,
            llm,
            embedding,
            key_slot,
            toggles,
            settings,
            config,
            shutdown,
        })
        .await;
    });

    log_spawned();
}

fn log_not_spawning_no_persistence() {
    tracing::debug!("session_judge: persistence not wired — not spawning");
}

fn log_client_build_failed(e: &crate::llm::LlmError) {
    tracing::warn!(error = %e, "session_judge: failed to build LLM client — not spawning");
}

fn log_spawned() {
    tracing::info!("session_judge: worker spawned");
}

struct SessionJudgeWorkerDeps {
    persist: PersistHandle,
    llm: OpenRouterClient,
    embedding: EmbeddingService,
    key_slot: crate::llm::key::OpenRouterKeySlot,
    toggles: Arc<ArcSwap<FeatureToggles>>,
    settings: Arc<ArcSwap<crate::settings::SettingsOverlay>>,
    config: SessionJudgeConfig,
    shutdown: CancellationToken,
}

async fn run_session_loop(deps: SessionJudgeWorkerDeps) {
    let SessionJudgeWorkerDeps {
        persist,
        llm,
        embedding,
        key_slot,
        toggles,
        settings,
        config,
        shutdown,
    } = deps;
    let poll = Duration::from_secs(config.poll_interval_secs);
    let mut interval = tokio::time::interval(poll);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    log_starting_poll_loop(&config);

    let mut breaker = CircuitBreaker::new(
        config.circuit_breaker_threshold,
        Duration::from_secs(config.circuit_breaker_cooldown_base_secs),
        Duration::from_secs(config.circuit_breaker_cooldown_max_secs),
    );

    let poll_deps = SessionPollDeps {
        persist: &persist,
        llm: &llm,
        embedding: &embedding,
        key_slot: &key_slot,
        config: &config,
    };

    loop {
        tokio::select! {
            biased;

            _ = shutdown.cancelled() => {
                log_shutdown_signalled();
                return;
            }

            _ = interval.tick() => {
                if !toggles.load().session_judge {
                    log_disabled_via_toggle();
                    return;
                }
                if !persist.holds_llm_role(LlmRole::SessionJudge, poll).await {
                    continue;
                }

                let runtime = resolve_session_judge_runtime(&settings.load(), &config);
                run_poll(&poll_deps, &runtime, &mut breaker).await;
            }
        }
    }
}

fn log_starting_poll_loop(config: &SessionJudgeConfig) {
    tracing::debug!(
        interval_secs = config.poll_interval_secs,
        model = %config.model,
        rubric_version = config.rubric_version,
        "session_judge: starting poll loop"
    );
}

fn log_shutdown_signalled() {
    tracing::debug!("session_judge: shutdown signalled, exiting");
}

fn log_disabled_via_toggle() {
    tracing::info!(
        "session_judge: disabled via feature toggle — exiting (a UI flip back on needs a restart)"
    );
}

pub struct SessionJudgeRuntime {
    pub model: String,
    pub system_prompt: String,
    pub batch_size: i64,
    pub min_session_age_secs: u64,
    pub max_turns_in_prompt: usize,
    pub max_chars_per_turn: usize,
    pub max_attempts_per_day: i64,
    pub bundle_id: Option<i64>,
    pub live_bundle_id: i64,
    pub v3_lookback_hours: i64,
    pub rejudge_daily_call_cap: i64,
    pub turn_judge_live_bundle_id: i64,
    pub turn_confidence_threshold: f32,
}

pub fn resolve_session_judge_runtime(
    overlay: &crate::settings::SettingsOverlay,
    cfg: &SessionJudgeConfig,
) -> SessionJudgeRuntime {
    let model = overlay
        .session_judge_model
        .as_deref()
        .filter(|m| !m.is_empty())
        .unwrap_or(cfg.model.as_str())
        .to_owned();
    let model = overlay
        .llm_provider
        .resolve(Some(crate::llm::provider::ModelTier::Fast), &model);

    let system_prompt = overlay
        .session_judge_system_prompt
        .as_deref()
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| {
            if cfg.system_prompt.trim().is_empty() {
                SESSION_JUDGE_SYSTEM_PROMPT
            } else {
                cfg.system_prompt.as_str()
            }
        })
        .to_owned();

    SessionJudgeRuntime {
        model,
        system_prompt,
        batch_size: overlay.session_judge_batch_size.unwrap_or(cfg.batch_size),
        min_session_age_secs: overlay
            .session_judge_min_session_age_secs
            .unwrap_or(cfg.min_session_age_secs),
        max_turns_in_prompt: overlay
            .session_judge_max_turns_in_prompt
            .unwrap_or(cfg.max_turns_in_prompt),
        max_chars_per_turn: usize::try_from(overlay.session_judge_max_chars_per_turn).unwrap_or(0),
        max_attempts_per_day: overlay
            .session_judge_max_attempts_per_day
            .unwrap_or(crate::store::persistence::judge::DEFAULT_MAX_ATTEMPTS_PER_DAY),
        bundle_id: overlay.bundles.session_judge,
        live_bundle_id: overlay.session_judge_live_bundle_id,
        v3_lookback_hours: overlay.session_judge_v3_lookback_hours,
        rejudge_daily_call_cap: overlay.session_judge_rejudge_daily_call_cap,
        turn_judge_live_bundle_id: overlay.turn_judge_live_bundle_id,
        turn_confidence_threshold: overlay.judge_worker_confidence_threshold.unwrap_or(0.5),
    }
}

#[cfg(any(test, feature = "test-seams"))]
pub async fn run_session_judge_once(state: &AppState) -> anyhow::Result<()> {
    crate::llm::limiter::install_runtime(
        state.settings_overlay.clone(),
        state.llm_usage_sink.clone(),
    );
    let Some(persist) = state.persist.clone() else {
        return Ok(());
    };
    let embedding = state.embedding_service();
    let config = state.config().session_judge.clone();
    let llm = crate::llm::OpenRouterClient::new(
        String::new(),
        config.model.clone(),
        Some(state.config().steerer.openrouter_base_url.clone()),
        Some(Duration::from_secs(config.request_timeout_secs)),
    )?;
    let llm = llm
        .with_attribution(LlmRole::SessionJudge, state.llm_usage_sink.as_ref())
        .with_live_settings(crate::settings::session_judge_live_source(
            &state.settings_overlay,
        ));
    let runtime = resolve_session_judge_runtime(&state.settings_overlay.load(), &config);
    let mut breaker = CircuitBreaker::new(
        config.circuit_breaker_threshold,
        Duration::from_secs(config.circuit_breaker_cooldown_base_secs),
        Duration::from_secs(config.circuit_breaker_cooldown_max_secs),
    );
    let deps = SessionPollDeps {
        persist: &persist,
        llm: &llm,
        embedding: &embedding,
        key_slot: &state.steerer_openrouter_key,
        config: &config,
    };
    run_poll(&deps, &runtime, &mut breaker).await;
    Ok(())
}
