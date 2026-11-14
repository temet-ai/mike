pub mod commit_message;
pub mod config;
pub mod effort;
pub mod seat_records;
pub mod stop;
pub mod stop_facts;
#[cfg(test)]
pub mod test_support;
pub mod text;
pub mod triage;
pub mod triage_rules;
pub mod wake;

use std::sync::Arc;

use arc_swap::ArcSwap;

pub use config::{DecisionsDefinition, DecisionsOverride, Mode, Point};

use crate::base_state::BaseState;
use crate::feature_toggles::FeatureToggles;
use crate::llm::OpenRouterClient;
use crate::llm::decisions::point::Decider;
use crate::settings::SettingsOverlay;
use crate::store::persistence::PersistHandle;

#[derive(Clone)]
pub struct FleetDecisions {
    base: OpenRouterClient,
    persist: PersistHandle,
    toggles: Arc<ArcSwap<FeatureToggles>>,
    overlay: Arc<ArcSwap<SettingsOverlay>>,
}

impl FleetDecisions {
    pub fn new(
        base: OpenRouterClient,
        persist: PersistHandle,
        toggles: Arc<ArcSwap<FeatureToggles>>,
        overlay: Arc<ArcSwap<SettingsOverlay>>,
    ) -> Self {
        Self {
            base,
            persist,
            toggles,
            overlay,
        }
    }

    pub fn from_state(state: &BaseState) -> Option<Self> {
        let persist = state.persist()?.clone();
        let config = &state.config.steerer;
        let base = OpenRouterClient::new(
            String::new(),
            config.model.clone(),
            Some(config.openrouter_base_url.clone()),
            Some(std::time::Duration::from_secs(config.request_timeout_secs)),
        )
        .ok()?;
        let keyed = crate::llm::key::effective_client(&base, &state.steerer_openrouter_key)?;
        Some(Self::new(
            keyed.with_attribution(crate::llm::LlmRole::Decision, state.llm_usage_sink.as_ref()),
            persist,
            state.feature_toggle_overlay.clone(),
            state.settings_overlay.clone(),
        ))
    }

    pub fn layer_on(&self) -> bool {
        self.toggles.load().decisions
    }

    pub fn persist(&self) -> &PersistHandle {
        &self.persist
    }

    pub fn decider(&self, point: Point, mode: Mode) -> Option<Decider> {
        if !self.layer_on() {
            return None;
        }
        let decision_mode = mode.decision_mode()?;
        Decider::with_mode(
            &self.base,
            &self.persist,
            &self.overlay.load(),
            point.name(),
            decision_mode,
        )
    }
}
