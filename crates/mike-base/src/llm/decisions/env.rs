use std::sync::{Arc, OnceLock};
use std::time::Duration;

use super::point::{Asked, Decider};
use crate::base_state::BaseState;
use crate::llm::key::effective_client;
use crate::llm::{LlmRole, OpenRouterClient};
use crate::settings::SettingsOverlay;
use crate::store::persistence::PersistHandle;

static BASE_CLIENT: OnceLock<Option<OpenRouterClient>> = OnceLock::new();

#[derive(Clone)]
pub struct DecisionEnv {
    client: OpenRouterClient,
    persist: PersistHandle,
    toggle_on: bool,
    overlay: Arc<SettingsOverlay>,
}

impl DecisionEnv {
    pub fn new(
        client: OpenRouterClient,
        persist: PersistHandle,
        toggle_on: bool,
        overlay: Arc<SettingsOverlay>,
    ) -> Self {
        Self {
            client,
            persist,
            toggle_on,
            overlay,
        }
    }

    pub fn decider(&self, point: &'static str) -> Option<Decider> {
        Decider::for_point(
            &self.client,
            &self.persist,
            self.toggle_on,
            &self.overlay,
            point,
        )
    }

    pub fn persist(&self) -> &PersistHandle {
        &self.persist
    }

    pub fn model(&self) -> String {
        self.overlay.decisions_model.clone()
    }
}

fn shared_client(base: &BaseState) -> Option<&'static OpenRouterClient> {
    BASE_CLIENT
        .get_or_init(|| {
            let steerer = &base.config.steerer;
            OpenRouterClient::new(
                String::new(),
                crate::settings::meta_decisions::DEFAULT_MODEL.to_owned(),
                Some(steerer.openrouter_base_url.clone()),
                Some(Duration::from_secs(steerer.request_timeout_secs)),
            )
            .ok()
        })
        .as_ref()
}

impl BaseState {
    pub fn decision_env(&self) -> Option<DecisionEnv> {
        if !self.feature_toggle_overlay.load().decisions {
            return None;
        }
        let persist = self.persist.clone()?;
        let shared = shared_client(self)?;
        let tagged = shared.with_attribution(LlmRole::Decision, self.llm_usage_sink.as_ref());
        let client = effective_client(&tagged, &self.steerer_openrouter_key)?;
        Some(DecisionEnv::new(
            client,
            persist,
            true,
            self.settings_overlay.load_full(),
        ))
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Pace {
    left: usize,
    stopped: bool,
}

impl Pace {
    pub fn new(max_calls: usize) -> Self {
        Self {
            left: max_calls,
            stopped: false,
        }
    }

    pub fn may_ask(&self) -> bool {
        !self.stopped && self.left > 0
    }

    pub fn spent(&mut self, asked: &Asked) {
        self.left = self.left.saturating_sub(1);
        if asked.failed() {
            self.stopped = true;
        }
    }

    pub fn stopped_early(&self) -> bool {
        self.stopped
    }
}

pub fn tail_chars(text: &str, max: usize) -> String {
    let total = text.chars().count();
    if total <= max {
        return text.trim().to_owned();
    }
    text.chars().skip(total - max).collect::<String>()
}

pub fn head_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pace_stops_after_the_budget_is_spent() {
        let mut pace = Pace::new(2);
        let ok = Asked::answered(std::collections::BTreeMap::new());
        pace.spent(&ok);
        assert!(pace.may_ask());
        pace.spent(&ok);
        assert!(!pace.may_ask());
        assert!(!pace.stopped_early());
    }

    #[test]
    fn pace_stops_early_on_a_failed_answer() {
        let mut pace = Pace::new(5);
        pace.spent(&Asked::failed_with(
            "OpenRouter call rate limit exceeded (120/min)",
        ));
        assert!(!pace.may_ask());
        assert!(pace.stopped_early());
    }

    #[test]
    fn tail_chars_keeps_the_end_of_long_text() {
        assert_eq!(tail_chars("abcdef", 3), "def");
        assert_eq!(tail_chars(" abc ", 10), "abc");
    }
}
