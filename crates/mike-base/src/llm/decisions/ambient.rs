use std::sync::{Arc, OnceLock};

use super::env::DecisionEnv;
use super::point::Decider;
use crate::base_state::BaseState;

type Source = Arc<dyn Fn() -> Option<DecisionEnv> + Send + Sync>;

static SOURCE: OnceLock<Source> = OnceLock::new();

pub fn install(state: &BaseState) {
    let state = state.clone();
    let source: Source = Arc::new(move || state.decision_env());
    if SOURCE.set(source).is_err() {
        tracing::debug!("decisions: the ambient source was already installed");
    }
}

pub fn env() -> Option<DecisionEnv> {
    SOURCE.get().and_then(|source| source())
}

pub fn decider(point: &'static str) -> Option<Decider> {
    env()?.decider(point)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decider_without_an_installed_source_is_none() {
        assert!(decider("gate_blame").is_none());
        assert!(env().is_none());
    }
}
