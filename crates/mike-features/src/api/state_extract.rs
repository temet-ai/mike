use axum::extract::FromRef;

use crate::base_state::BaseState;
use crate::launch::state::LaunchState;
use crate::semantic::KnowledgeState;
use crate::state::AppState;

impl FromRef<AppState> for BaseState {
    fn from_ref(state: &AppState) -> BaseState {
        state.base.clone()
    }
}

impl FromRef<AppState> for LaunchState {
    fn from_ref(state: &AppState) -> LaunchState {
        state.launch_state()
    }
}

impl FromRef<AppState> for KnowledgeState {
    fn from_ref(state: &AppState) -> KnowledgeState {
        state.knowledge_state()
    }
}
