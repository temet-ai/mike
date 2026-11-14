use std::time::Duration;

use crate::circuit_breaker::CircuitBreaker;
use crate::settings::resolve_judge_worker_llm_settings;
use crate::state::AppState;

use super::live_shared::LiveJudgeShared;
use super::live_tick::{LiveTick, LiveTickReport, run_live_tick};
use super::turn_sweep::{JudgeSweepParams, resolve_turn_judge_runtime, run_judge_sweep};

pub async fn run_judge_once(state: &AppState) -> anyhow::Result<usize> {
    let judge_cfg = state.config().steerer.judge.clone();
    let mut breaker = CircuitBreaker::new(
        judge_cfg.circuit_breaker_threshold,
        Duration::from_secs(judge_cfg.circuit_breaker_cooldown_base_secs),
        Duration::from_secs(judge_cfg.circuit_breaker_cooldown_max_secs),
    );
    run_judge_once_with_breaker(state, &mut breaker).await
}

pub async fn run_judge_once_with_breaker(
    state: &AppState,
    breaker: &mut CircuitBreaker,
) -> anyhow::Result<usize> {
    crate::llm::limiter::install_runtime(
        state.settings_overlay.clone(),
        state.llm_usage_sink.clone(),
    );
    let Some(persist) = state.persist.clone() else {
        return Ok(0);
    };
    let Some(llm) = state.judge_llm.clone() else {
        return Ok(0);
    };
    let judge_cfg = state.config().steerer.judge.clone();
    let live_settings = state.settings_overlay.load();
    let runtime =
        resolve_turn_judge_runtime(&live_settings, &judge_cfg, &state.config().steerer.model);
    let Some(effective_llm) =
        crate::llm::key::effective_client_for(&llm, &state.steerer_openrouter_key, &runtime.model)
    else {
        return Ok(0);
    };
    let effective_llm =
        effective_llm.with_runtime_settings(resolve_judge_worker_llm_settings(&live_settings));

    let params = JudgeSweepParams {
        model_label: &runtime.model,
        system_prompt: &runtime.system_prompt,
        rubric_version: &runtime.rubric_version,
        batch: runtime.batch_size,
        threshold: runtime.confidence_threshold,
        backoff_base_secs: judge_cfg.backoff_base_secs,
        backoff_max_secs: judge_cfg.backoff_max_secs,
        bundle_id: live_settings.bundles.turn_judge,
        decisions: crate::llm::decisions::point::Decider::for_point(
            &effective_llm,
            &persist,
            state.feature_toggle_overlay.load().decisions,
            &live_settings,
            crate::analysis::judge::jev::POINT,
        ),
    };

    Ok(run_judge_sweep(&persist, &effective_llm, breaker, &params)
        .await?
        .labeled)
}

pub async fn run_judge_sweeps_and_report_breaker_tripped(
    state: &AppState,
    sweeps: u32,
) -> anyhow::Result<bool> {
    let judge_cfg = state.config().steerer.judge.clone();
    let mut breaker = CircuitBreaker::new(
        judge_cfg.circuit_breaker_threshold,
        Duration::from_secs(judge_cfg.circuit_breaker_cooldown_base_secs),
        Duration::from_secs(judge_cfg.circuit_breaker_cooldown_max_secs),
    );
    for _ in 0..sweeps {
        run_judge_once_with_breaker(state, &mut breaker).await?;
        if breaker.should_skip(std::time::Instant::now()) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub async fn run_live_judge_once(state: &AppState) -> anyhow::Result<(usize, usize)> {
    crate::llm::limiter::install_runtime(
        state.settings_overlay.clone(),
        state.llm_usage_sink.clone(),
    );
    let (Some(persist), Some(llm)) = (state.persist.clone(), state.judge_llm.clone()) else {
        return Ok((0, 0));
    };
    let Some(effective_llm) = crate::llm::key::keyed_client(&llm, &state.steerer_openrouter_key)
    else {
        return Ok((0, 0));
    };
    let judge_cfg = state.config().steerer.judge.clone();
    let mut breaker = CircuitBreaker::new(
        judge_cfg.circuit_breaker_threshold,
        Duration::from_secs(judge_cfg.circuit_breaker_cooldown_base_secs),
        Duration::from_secs(judge_cfg.circuit_breaker_cooldown_max_secs),
    );
    let overlay = state.settings_overlay.load();
    let shared = LiveJudgeShared::default();
    let tick = LiveTick {
        persist: &persist,
        llm: &effective_llm,
        shared: &shared,
        overlay: &overlay,
        judge_cfg: &judge_cfg,
        decisions_on: state.feature_toggle_overlay.load().decisions,
    };
    let LiveTickReport {
        promoted, rejudged, ..
    } = run_live_tick(tick, &mut breaker).await;
    Ok((promoted, rejudged))
}

pub fn default_v2_bundle_components(model: &str) -> serde_json::Value {
    use crate::analysis::judge::input_v2::INPUT_BUILDER_V2;
    use crate::analysis::judge::rubric::{V2_2026_09_23, effective_rubric};
    super::v2_judge::V2JudgeConfig {
        rubric: effective_rubric(&V2_2026_09_23, None),
        builder: INPUT_BUILDER_V2,
        model: model.to_owned(),
        llm_settings: crate::llm::LlmRuntimeSettings::default(),
        slices: Vec::new(),
    }
    .to_bundle_components()
}

pub async fn run_immediate_once(
    state: &AppState,
    session_uuid: &str,
    human_prompt: &str,
) -> anyhow::Result<usize> {
    crate::llm::limiter::install_runtime(
        state.settings_overlay.clone(),
        state.llm_usage_sink.clone(),
    );
    let (Some(persist), Some(llm)) = (state.persist.clone(), state.judge_llm.clone()) else {
        return Ok(0);
    };
    let judge_cfg = state.config().steerer.judge.clone();
    let immediate = super::immediate::ImmediateJudge {
        persist,
        llm,
        key_slot: state.steerer_openrouter_key.clone(),
        toggles: state.feature_toggle_overlay.clone(),
        settings: state.settings_overlay.clone(),
        shared: LiveJudgeShared::default(),
        judge_cfg: judge_cfg.clone(),
        shutdown: state.shutdown(),
    };
    let event = crate::store::models::ProxyEvent::OperatorTurnClosed {
        closed_turn_id: 0,
        session_id: session_uuid.to_owned(),
        new_request_id: crate::store::models::RequestId::from_raw(0),
        human_prompt: human_prompt.to_owned(),
    };
    let Some((session, target)) = immediate.target_for(&event) else {
        return Ok(0);
    };
    let mut breaker = CircuitBreaker::new(
        judge_cfg.circuit_breaker_threshold,
        Duration::from_secs(judge_cfg.circuit_breaker_cooldown_base_secs),
        Duration::from_secs(judge_cfg.circuit_breaker_cooldown_max_secs),
    );
    Ok(immediate
        .judge_session(&session, target, &mut breaker)
        .await)
}
