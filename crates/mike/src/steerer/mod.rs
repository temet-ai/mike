mod ac2_demo;
pub(crate) mod anchor;
pub(crate) mod arming;
pub(crate) mod criteria_contract;
pub(crate) mod enrolment_hook;
pub(crate) mod fleet;
pub(crate) mod gate;
pub(crate) mod guard;
#[cfg(any(test, feature = "test-seams"))]
pub use guard::test_seams as guard_test_seams;
pub(crate) mod inject;
pub(crate) mod intake_criteria;
pub(crate) mod model_catalogue;
pub(crate) mod note_policy;
pub(crate) mod refocus;
pub(crate) mod registry;
pub(crate) mod reply_label;
pub(crate) mod rollout_params;
pub(crate) mod service;
pub(crate) mod stage;
pub(crate) mod steer_author;
pub mod structural;
pub mod verdict;
pub mod verifier;

use crate::state::AppState;

pub fn spawn_steerer_fleet(state: &AppState) {
    crate::llm::limiter::install_runtime(
        state.settings_overlay.clone(),
        state.llm_usage_sink.clone(),
    );

    if !state.feature_toggle_overlay.load().steerer {
        return;
    }

    let config = state.config.steerer.clone();

    let Some(llm) = build_openrouter_client(state, &config) else {
        return;
    };

    let args = fleet_args_from_state(state, config, llm);
    state.tracker.spawn(async move {
        fleet::run_fleet(args).await;
    });

    tracing::info!("steerer fleet: spawned");
}

fn build_openrouter_client(
    state: &AppState,
    config: &crate::config::steerer::SteererConfig,
) -> Option<crate::llm::OpenRouterClient> {
    match crate::llm::OpenRouterClient::new(
        String::new(),
        config.model.clone(),
        Some(config.openrouter_base_url.clone()),
        Some(std::time::Duration::from_secs(config.request_timeout_secs)),
    ) {
        Ok(client) => Some(
            client
                .with_attribution(crate::llm::LlmRole::Steerer, state.llm_usage_sink.as_ref())
                .with_live_settings(crate::settings::steerer_live_source(
                    &state.settings_overlay,
                )),
        ),
        Err(e) => {
            tracing::warn!(error = %e, "steerer fleet: failed to build LLM client — not spawning");
            None
        }
    }
}

fn fleet_args_from_state(
    state: &AppState,
    config: crate::config::steerer::SteererConfig,
    llm: crate::llm::OpenRouterClient,
) -> fleet::FleetArgs {
    fleet::FleetArgs {
        rx: state.event_tx.subscribe(),
        event_tx: state.event_tx.clone(),
        store: state.store.clone(),
        persist: state.persist.clone(),
        steerer_registry: state.steerer_registry.clone(),
        toggles: state.feature_toggle_overlay.clone(),
        llm,
        key_slot: state.steerer_openrouter_key.clone(),
        config,
        shutdown: state.shutdown.clone(),
        remote_control_streams: state.remote_control_streams.clone(),
        injection_gate: state.injection_gate.clone(),
        settings: state.settings_overlay.clone(),
    }
}
